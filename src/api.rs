use std::str::FromStr;

use ureq::http::Method;
use ureq::http::uri::PathAndQuery;

use crate::cli::ApiArgs;
use crate::failure::Failure;
use crate::http::Client;
use crate::request;
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
    let body = args.body.as_deref().map(request::body_bytes).transpose()?;

    let mut client = Client::signed_in()?;
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
