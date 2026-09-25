use serde::Deserialize;
use ureq::Agent;

use crate::failure::{ErrorCode, Failure};
use crate::http;
use crate::oauth::CLIENT_ID;

/// Where Auth answers for one Compliance host, learnt from the host's Protected Resource Metadata (RFC 9728)
/// and then Auth's own discovery document.
pub struct AuthServer {
    /// The REST API resource every token is bound to (RFC 8707).
    pub resource: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub device_authorization_endpoint: String,
    pub revocation_endpoint: String,
}

impl AuthServer {
    /// `fields` after the two every request to Auth names: the CLI's client id and the API resource.
    pub fn form<'a>(&'a self, fields: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut form = vec![
            ("client_id", CLIENT_ID),
            ("resource", self.resource.as_str()),
        ];
        form.extend_from_slice(fields);
        form
    }
}

#[derive(Deserialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
}

#[derive(Deserialize)]
struct AuthMetadata {
    authorization_endpoint: String,
    token_endpoint: String,
    device_authorization_endpoint: String,
    revocation_endpoint: String,
}

pub fn discover(agent: &Agent, host: &str) -> Result<AuthServer, Failure> {
    let metadata: ProtectedResourceMetadata = get_json(
        agent,
        &format!("{host}/.well-known/oauth-protected-resource/api"),
    )?;
    let Some(auth) = metadata.authorization_servers.first() else {
        return Err(Failure::local(
            ErrorCode::Discovery,
            format!("{host} names no authorization server in its Protected Resource Metadata."),
        ));
    };
    let auth: AuthMetadata = get_json(
        agent,
        &format!(
            "{}/.well-known/openid-configuration",
            auth.trim_end_matches('/')
        ),
    )?;
    Ok(AuthServer {
        resource: metadata.resource,
        authorization_endpoint: auth.authorization_endpoint,
        token_endpoint: auth.token_endpoint,
        device_authorization_endpoint: auth.device_authorization_endpoint,
        revocation_endpoint: auth.revocation_endpoint,
    })
}

fn get_json<T: serde::de::DeserializeOwned>(agent: &Agent, url: &str) -> Result<T, Failure> {
    let (status, bytes) = http::read_answer(url, agent.get(url).call())?;
    if !status.is_success() {
        return Err(Failure::local(
            ErrorCode::Discovery,
            format!("{url} answered {status}."),
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        Failure::local(
            ErrorCode::Discovery,
            format!("{url} is not the expected metadata: {error}"),
        )
    })
}
