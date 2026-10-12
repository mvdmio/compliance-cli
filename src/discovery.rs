use serde::Deserialize;
use ureq::Agent;

use crate::failure::{ErrorCode, Failure};
use crate::http;
use crate::oauth::CLIENT_ID;
use crate::products;

/// Where Auth answers, learnt from Auth's own discovery document, and the REST API resource its tokens are
/// bound to.
pub struct AuthServer {
    /// The first listed Product's REST API resource (RFC 8707). Auth makes the one connection reach every
    /// listed Product. A refresh goes without it when the Product list cannot be read.
    pub resource: Option<String>,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub device_authorization_endpoint: String,
}

impl AuthServer {
    /// `fields` after the two every request to Auth names: the CLI's client id and, when known, the API resource.
    pub fn form<'a>(&'a self, fields: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut form = vec![("client_id", CLIENT_ID)];
        if let Some(resource) = &self.resource {
            form.push(("resource", resource.as_str()));
        }
        form.extend_from_slice(fields);
        form
    }
}

#[derive(Deserialize)]
struct AuthMetadata {
    authorization_endpoint: String,
    token_endpoint: String,
    device_authorization_endpoint: String,
    revocation_endpoint: String,
}

fn metadata(agent: &Agent, auth: &str) -> Result<AuthMetadata, Failure> {
    get_json(agent, &format!("{auth}/.well-known/openid-configuration"))
}

/// Auth's revocation endpoint alone: revoking names no Product, so it needs no Product list.
pub fn revocation_endpoint(agent: &Agent, auth: &str) -> Result<String, Failure> {
    Ok(metadata(agent, auth)?.revocation_endpoint)
}

/// Auth's endpoints and the resource a sign-in names, which needs the Product list.
pub fn discover(agent: &Agent, auth: &str) -> Result<AuthServer, Failure> {
    let resource = Some(products::sign_in_resource(auth)?);
    server(agent, auth, resource)
}

/// Auth's endpoints for a refresh. The refresh needs only Auth's discovery document: it names the resource when
/// the Product list can be read, and goes without it when not.
pub fn discover_for_refresh(agent: &Agent, auth: &str) -> Result<AuthServer, Failure> {
    let resource = products::sign_in_resource(auth).ok();
    server(agent, auth, resource)
}

fn server(agent: &Agent, auth: &str, resource: Option<String>) -> Result<AuthServer, Failure> {
    let metadata = metadata(agent, auth)?;
    Ok(AuthServer {
        resource,
        authorization_endpoint: metadata.authorization_endpoint,
        token_endpoint: metadata.token_endpoint,
        device_authorization_endpoint: metadata.device_authorization_endpoint,
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
