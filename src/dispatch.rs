//! Turns the arguments of a generated command into its request.

use std::path::PathBuf;

use clap::ArgMatches;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Map, Value};

use crate::api;
use crate::config;
use crate::credential;
use crate::failure::Failure;
use crate::generated::{self, BODY, OUT};
use crate::http::Client;
use crate::openapi::Operation;
use crate::response;

/// RFC 3986 unreserved characters stay as they are in a path segment.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
/// Also keeps the characters OData expressions use.
const QUERY: &AsciiSet = &SEGMENT
    .remove(b'$')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b',')
    .remove(b':')
    .remove(b'/')
    .remove(b'@')
    .remove(b'!')
    .remove(b'*');

pub fn run(operation: &Operation, matches: &ArgMatches) -> Result<(), Failure> {
    let target = target(operation, matches);
    let body = body(operation, matches)?;
    let out = matches.get_one::<PathBuf>(OUT);

    let host = config::host();
    let mut client = Client::new(host.clone(), credential::require(&host)?);
    let answer = client.send(&operation.method, &target, body.as_deref())?;
    response::print_response(answer, out.map(PathBuf::as_path))
}

/// The path with its placeholders filled, and the query string.
fn target(operation: &Operation, matches: &ArgMatches) -> String {
    let mut path = operation.path.clone();
    for parameter in &operation.path_params {
        if let Some(value) = matches.get_one::<Value>(&generated::path_id(parameter)) {
            let segment = utf8_percent_encode(&text(value), SEGMENT).to_string();
            path = path.replace(&format!("{{{}}}", parameter.name), &segment);
        }
    }
    let mut query = Vec::new();
    for parameter in &operation.query_params {
        let values = matches
            .get_many::<Value>(&generated::query_id(parameter))
            .into_iter()
            .flatten();
        for value in values {
            query.push(format!(
                "{}={}",
                utf8_percent_encode(&parameter.name, QUERY),
                utf8_percent_encode(&text(value), QUERY)
            ));
        }
    }
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }
    path
}

/// `--body` with the field options set on top of it. A required body with no input is `{}`.
fn body(operation: &Operation, matches: &ArgMatches) -> Result<Option<Vec<u8>>, Failure> {
    let Some(body) = &operation.body else {
        return Ok(None);
    };
    let whole = matches
        .get_one::<String>(BODY)
        .map(|text| api::json_value(text))
        .transpose()?;
    let mut fields = Map::new();
    for field in &body.fields {
        let id = generated::field_id(field);
        let value = if field.schema.array {
            matches
                .get_many::<Value>(&id)
                .map(|values| Value::Array(values.cloned().collect()))
        } else {
            matches.get_one::<Value>(&id).cloned()
        };
        if let Some(value) = value {
            fields.insert(field.name.clone(), value);
        }
    }

    let value = match whole {
        None if fields.is_empty() && !body.required => return Ok(None),
        None => Value::Object(fields),
        Some(Value::Object(mut whole)) => {
            whole.extend(fields);
            Value::Object(whole)
        }
        Some(_) if !fields.is_empty() => {
            return Err(Failure::Usage(
                "--body must be a JSON object when field options are given too.".to_string(),
            ));
        }
        Some(whole) => whole,
    };
    Ok(Some(value.to_string().into_bytes()))
}

/// A string as it is; any other value as JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
