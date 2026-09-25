use std::thread;
use std::time::{Duration, SystemTime};

use ureq::Agent;
use ureq::http::{HeaderMap, Method, Request, Response, header};

use crate::credential::{self, Credential};
use crate::failure::Failure;

const USER_AGENT: &str = concat!("compliance-cli/", env!("CARGO_PKG_VERSION"));
const MAX_RATE_LIMIT_RETRIES: usize = 3;
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(1);
/// A stored access token this close to its expiry is refreshed before the call.
const REFRESH_MARGIN_SECONDS: u64 = 60;

/// The one HTTP agent every request uses: the CLI's User-Agent, and every status returned as a response.
pub fn agent() -> Agent {
    Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .new_agent()
}

/// A request that never got an answer: `config` for an address that does not parse, else `network`.
pub fn transport_failure(url: &str, error: ureq::Error) -> Failure {
    match error {
        ureq::Error::BadUri(_) | ureq::Error::Http(_) => {
            Failure::local("config", format!("{url} is not a valid address: {error}"))
        }
        _ => Failure::local("network", format!("{url}: {error}")),
    }
}

/// Calls one Compliance host with one credential. A stored sign-in is refreshed when its access token is about to
/// expire, and once more on a 401.
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

    /// Sends `method` to `path` (relative to the host, query string included) with an optional JSON body. A 429
    /// waits for `Retry-After` and retries, up to `MAX_RATE_LIMIT_RETRIES`; the last answer is returned whatever
    /// its status.
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
        let result = match json_body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body)
                .map(|request| self.agent.run(request)),
            None => request.body(()).map(|request| self.agent.run(request)),
        };
        result
            .map_err(|error| {
                Failure::local(
                    "config",
                    format!("COMPLIANCE_URL gives {url}, which is not a valid address: {error}"),
                )
            })?
            .map_err(|error| Failure::local("network", format!("{url}: {error}")))
    }
}

/// `Retry-After` as seconds or an HTTP date (RFC 9110 §10.2.3).
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
