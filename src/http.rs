use std::thread;
use std::time::{Duration, SystemTime};

use ureq::Agent;
use ureq::http::{HeaderMap, Method, Request, Response, StatusCode, header};

use crate::config;
use crate::credential::{self, Credential};
use crate::failure::{ErrorCode, Failure};

const USER_AGENT: &str = concat!("compliance-cli/", env!("CARGO_PKG_VERSION"));
const MAX_RATE_LIMIT_RETRIES: usize = 3;
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(1);
const REFRESH_MARGIN_SECONDS: u64 = 60;

pub fn agent() -> Agent {
    Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .new_agent()
}

pub fn transport_failure(url: &str, error: ureq::Error) -> Failure {
    match error {
        ureq::Error::BadUri(_) | ureq::Error::Http(_) => Failure::local(
            ErrorCode::Config,
            format!("{url} is not a valid address: {error}"),
        ),
        _ => Failure::local(ErrorCode::Network, format!("{url}: {error}")),
    }
}

pub fn read_answer(
    url: &str,
    result: Result<Response<ureq::Body>, ureq::Error>,
) -> Result<(StatusCode, Vec<u8>), Failure> {
    let mut response = result.map_err(|error| transport_failure(url, error))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(u64::MAX)
        .read_to_vec()
        .map_err(|error| transport_failure(url, error))?;
    Ok((response.status(), bytes))
}

pub struct Client {
    agent: Agent,
    host: String,
    credential: Credential,
}

impl Client {
    pub fn new(host: String, credential: Credential) -> Self {
        Client {
            agent: agent(),
            host,
            credential,
        }
    }

    pub fn signed_in() -> Result<Self, Failure> {
        let host = config::host();
        let credential = credential::require(&host)?;
        Ok(Client::new(host, credential))
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn credential(&self) -> &Credential {
        &self.credential
    }

    pub fn send(
        &mut self,
        method: &Method,
        path: &str,
        json_body: Option<&[u8]>,
    ) -> Result<Response<ureq::Body>, Failure> {
        let refresh_first = matches!(&self.credential, Credential::AgentConnection(sign_in)
            if sign_in.expires_within(REFRESH_MARGIN_SECONDS));
        if refresh_first {
            self.refresh()?;
        }
        let response = self.send_rate_limited(method, path, json_body)?;
        if response.status() == 401
            && !refresh_first
            && matches!(self.credential, Credential::AgentConnection(_))
        {
            self.refresh()?;
            return self.send_rate_limited(method, path, json_body);
        }
        Ok(response)
    }

    fn refresh(&mut self) -> Result<(), Failure> {
        if let Credential::AgentConnection(sign_in) = &self.credential {
            let refreshed = credential::refresh(&self.agent, &self.host, sign_in)?;
            self.credential = Credential::AgentConnection(refreshed);
        }
        Ok(())
    }

    fn send_rate_limited(
        &self,
        method: &Method,
        path: &str,
        json_body: Option<&[u8]>,
    ) -> Result<Response<ureq::Body>, Failure> {
        let url = format!("{}{}", self.host, path);
        let mut retries = 0;
        loop {
            let response = self.send_once(method, &url, json_body)?;
            if response.status() != 429 || retries == MAX_RATE_LIMIT_RETRIES {
                return Ok(response);
            }
            retries += 1;
            thread::sleep(retry_after(response.headers()));
        }
    }

    fn send_once(
        &self,
        method: &Method,
        url: &str,
        json_body: Option<&[u8]>,
    ) -> Result<Response<ureq::Body>, Failure> {
        let request = Request::builder().method(method.clone()).uri(url).header(
            header::AUTHORIZATION,
            format!("Bearer {}", self.credential.bearer()),
        );
        match json_body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .map_err(ureq::Error::from)
                .and_then(|request| self.agent.run(request)),
            None => request
                .body(())
                .map_err(ureq::Error::from)
                .and_then(|request| self.agent.run(request)),
        }
        .map_err(|error| transport_failure(url, error))
    }
}

fn retry_after(headers: &HeaderMap) -> Duration {
    let Some(value) = headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    else {
        return DEFAULT_RETRY_AFTER;
    };
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Duration::from_secs(seconds);
    }
    match httpdate::parse_http_date(value.trim()) {
        Ok(at) => at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
        Err(_) => DEFAULT_RETRY_AFTER,
    }
}
