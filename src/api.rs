use std::fs;
use std::str::FromStr;

use ureq::http::Method;
use ureq::http::uri::PathAndQuery;

use crate::cli::ApiArgs;
use crate::config;
use crate::credential;
use crate::failure::Failure;
use crate::http::Client;
use crate::response;

/// `compliance api <method> <path>`: one raw call with the current credential.
pub fn run(args: ApiArgs) -> Result<(), Failure> {
    let method = method(&args.method)?;
    if !args.path.starts_with('/') || PathAndQuery::from_str(&args.path).is_err() {
        return Err(Failure::Usage(format!(
            "The path must start with / and hold no spaces or control characters, such as /api/v1/risks, not {}.",
            args.path
        )));
    }
    let body = args.body.as_deref().map(json_body).transpose()?;

    let host = config::host();
    let mut client = Client::new(host.clone(), credential::require(&host)?);
    let answer = client.send(&method, &args.path, body.as_deref())?;
    response::print_response(answer, args.out.as_deref())
}

fn method(text: &str) -> Result<Method, Failure> {
    match text.to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "PATCH" => Ok(Method::PATCH),
        "DELETE" => Ok(Method::DELETE),
        "HEAD" => Ok(Method::HEAD),
        "OPTIONS" => Ok(Method::OPTIONS),
        _ => Err(Failure::Usage(format!(
            "Unknown method {text}. Use GET, POST, PUT, PATCH, DELETE, HEAD, or OPTIONS."
        ))),
    }
}

/// `--body`: inline JSON, or `@path` to read it from a file. It must parse, so a typo fails before the call.
fn json_body(text: &str) -> Result<Vec<u8>, Failure> {
    let bytes = body_text(text)?;
    parse_body(&bytes)?;
    Ok(bytes)
}

/// `--body` of a generated command, parsed.
pub fn json_value(text: &str) -> Result<serde_json::Value, Failure> {
    parse_body(&body_text(text)?)
}

fn body_text(text: &str) -> Result<Vec<u8>, Failure> {
    match text.strip_prefix('@') {
        Some(path) => {
            fs::read(path).map_err(|error| Failure::local("file", format!("{path}: {error}")))
        }
        None => Ok(text.as_bytes().to_vec()),
    }
}

fn parse_body(bytes: &[u8]) -> Result<serde_json::Value, Failure> {
    serde_json::from_slice(bytes)
        .map_err(|error| Failure::Usage(format!("--body is not valid JSON: {error}")))
}
