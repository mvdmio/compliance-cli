use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::Value;
use ureq::Agent;

use crate::discovery::AuthServer;
use crate::failure::Failure;
use crate::http;
use crate::store::{self, SignIn, User};

/// The CLI's pre-registered public client at Auth.
pub const CLIENT_ID: &str = "mvdmio.compliance_cli";

/// Asked for on authorize and device requests, so the token response carries an `id_token` with the User.
pub const SCOPE: &str = "openid";

/// An OAuth error answer (RFC 6749 §5.2).
pub struct OAuthError {
    pub error: String,
    pub description: Option<String>,
}

impl OAuthError {
    pub fn into_failure(self) -> Failure {
        let code = match self.error.as_str() {
            "access_denied" => "access-denied",
            "expired_token" => "expired-token",
            _ => "auth",
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

/// Posts a form to an Auth endpoint: the JSON body of a 2xx, or the OAuth error of anything else.
pub fn post_form(
    agent: &Agent,
    url: &str,
    form: &[(&str, &str)],
) -> Result<Result<Value, OAuthError>, Failure> {
    let mut response = agent
        .post(url)
        .send_form(form.iter().copied())
        .map_err(|error| http::transport_failure(url, error))?;
    let status = response.status();
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|error| http::transport_failure(url, error))?;
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

/// Asks the token endpoint for tokens: the new sign-in, or the OAuth error. `previous` keeps the refresh token
/// and the User when the answer leaves them out.
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
            "auth",
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

/// Revokes a token (RFC 7009). Auth answers 200 for a token it no longer knows, too.
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

/// The User named in an `id_token`. The token came straight from Auth's token endpoint over the connection the
/// CLI opened, so its claims are read without checking the signature (OpenID Connect Core §3.1.3.7).
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

/// A random URL-safe string with `bytes` bytes of entropy, for PKCE and `state`.
pub fn random_text(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .expect("the system random source works");
    URL_SAFE_NO_PAD.encode(buffer)
}

/// The PKCE S256 challenge for a verifier (RFC 7636 §4.2).
pub fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ))
}
