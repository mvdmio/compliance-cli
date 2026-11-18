use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::Value;
use ureq::Agent;

use crate::discovery::AuthServer;
use crate::failure::{ErrorCode, Failure};
use crate::http;
use crate::store::{self, SignIn, User};

pub const CLIENT_ID: &str = "mvdmio.compliance_cli";

pub const SCOPE: &str = "openid";

pub struct OAuthError {
    pub error: String,
    pub description: Option<String>,
}

impl OAuthError {
    pub fn into_failure(self) -> Failure {
        let code = match self.error.as_str() {
            "access_denied" => ErrorCode::AccessDenied,
            "expired_token" => ErrorCode::ExpiredToken,
            _ => ErrorCode::Auth,
        };
        let message = match self.description {
            Some(description) => format!("Auth answered {}: {description}", self.error),
            None => format!("Auth answered {}.", self.error),
        };
        Failure::local(code, message)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

pub fn post_form(
    agent: &Agent,
    url: &str,
    form: &[(&str, &str)],
) -> Result<Result<Value, OAuthError>, Failure> {
    let (status, bytes) = http::read_answer(url, agent.post(url).send_form(form.iter().copied()))?;
    let body: Option<Value> = serde_json::from_slice(&bytes).ok();
    if status.is_success() {
        return Ok(Ok(body.unwrap_or(Value::Null)));
    }
    let text = |field: &str| {
        body.as_ref()
            .and_then(|body| body[field].as_str())
            .map(str::to_string)
    };
    Ok(Err(OAuthError {
        error: text("error").unwrap_or_else(|| format!("HTTP {status}")),
        description: text("error_description"),
    }))
}

pub fn request_tokens(
    agent: &Agent,
    server: &AuthServer,
    form: &[(&str, &str)],
    previous: Option<&SignIn>,
) -> Result<Result<SignIn, OAuthError>, Failure> {
    let body = match post_form(agent, &server.token_endpoint, form)? {
        Ok(body) => body,
        Err(error) => return Ok(Err(error)),
    };
    let tokens: TokenResponse = serde_json::from_value(body).map_err(|error| {
        Failure::local(
            ErrorCode::Auth,
            format!("The token endpoint's answer is not a token response: {error}"),
        )
    })?;
    Ok(Ok(SignIn {
        access_token: tokens.access_token,
        expires_at: store::now() + tokens.expires_in.unwrap_or(3600),
        refresh_token: tokens
            .refresh_token
            .or_else(|| previous.and_then(|sign_in| sign_in.refresh_token.clone())),
        user: tokens
            .id_token
            .as_deref()
            .and_then(user_of)
            .or_else(|| previous.and_then(|sign_in| sign_in.user.clone())),
    }))
}

pub fn revoke(agent: &Agent, server: &AuthServer, sign_in: &SignIn) -> Result<(), Failure> {
    let (token, hint) = match &sign_in.refresh_token {
        Some(refresh_token) => (refresh_token.as_str(), "refresh_token"),
        None => (sign_in.access_token.as_str(), "access_token"),
    };
    let form = [
        ("client_id", CLIENT_ID),
        ("token", token),
        ("token_type_hint", hint),
    ];
    post_form(agent, &server.revocation_endpoint, &form)?.map_err(OAuthError::into_failure)?;
    Ok(())
}

fn user_of(id_token: &str) -> Option<User> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    let text = |claim: &str| claims[claim].as_str().map(str::to_string);
    Some(User {
        name: text("name"),
        email: text("email"),
    })
}

pub fn random_text(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .expect("the system random source works");
    URL_SAFE_NO_PAD.encode(buffer)
}

pub fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ))
}
