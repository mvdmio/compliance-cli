//! Turns the arguments of a generated command into its request.

use std::path::PathBuf;

use clap::ArgMatches;
use percent_encoding::{AsciiSet, utf8_percent_encode};
use serde_json::{Map, Value};

use crate::failure::Failure;
use crate::generated::{self, BODY, FILE, Location, OUT, UploadSlot};
use crate::http::Client;
use crate::openapi::{Operation, Parameter};
use crate::request::{self, UNRESERVED, path_segment};
use crate::response;
use crate::upload;

/// Also keeps the characters OData expressions use.
const QUERY: &AsciiSet = &UNRESERVED
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

/// Uploads each `--file` through an Upload link first; the call then carries the upload ids.
pub fn run(operation: &Operation, matches: &ArgMatches) -> Result<(), Failure> {
    // Only an operation with an upload target has `--file`.
    let files: Vec<&PathBuf> = matches
        .try_get_many::<PathBuf>(FILE)
        .ok()
        .flatten()
        .into_iter()
        .flatten()
        .collect();
    let upload = generated::upload_slot(operation).filter(|_| !files.is_empty());
    let upload_field = upload
        .as_ref()
        .filter(|slot| slot.location == Location::Body)
        .map(|slot| slot.parameter);
    let mut body = body(operation, matches, upload_field)?;
    // Only a download has `--out`.
    let out = matches.try_get_one::<PathBuf>(OUT).ok().flatten();

    let mut client = Client::signed_in()?;
    let uploaded = match &upload {
        Some(slot) => Some((slot, upload::send_all(&mut client, slot, &files)?)),
        None => None,
    };
    if let (Some(field), Some((_, ids)), Some(Value::Object(fields))) =
        (upload_field, &uploaded, &mut body)
    {
        fields.insert(field.name.clone(), ids.clone());
    }

    let target = target(operation, matches, uploaded.as_ref());
    let body = body.map(|value| value.to_string().into_bytes());
    let answer = client.send(&operation.method, &target, body.as_deref())?;
    response::print_response(answer, out.map(PathBuf::as_path))
}

/// The path with its placeholders filled, and the query string. `uploaded` stands in for its slot's values.
fn target(
    operation: &Operation,
    matches: &ArgMatches,
    uploaded: Option<&(&UploadSlot, Value)>,
) -> String {
    let mut path = operation.path.clone();
    for parameter in &operation.path_params {
        if let Some(value) = matches.get_one::<Value>(&generated::path_id(parameter)) {
            let segment = path_segment(&text(value));
            path = path.replace(&format!("{{{}}}", parameter.name), &segment);
        }
    }
    let mut query = Vec::new();
    for parameter in &operation.query_params {
        let values: Vec<&Value> = match uploaded {
            Some((slot, ids)) if slot.is(parameter, Location::Query) => match ids {
                Value::Array(ids) => ids.iter().collect(),
                id => vec![id],
            },
            _ => matches
                .get_many::<Value>(&generated::query_id(parameter))
                .into_iter()
                .flatten()
                .collect(),
        };
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

/// `--body` with the field options set on top of it. A required body with no input is `{}`. `upload` is the
/// field `--file` fills later, which counts as a field option.
fn body(
    operation: &Operation,
    matches: &ArgMatches,
    upload: Option<&Parameter>,
) -> Result<Option<Value>, Failure> {
    let Some(body) = &operation.body else {
        return Ok(None);
    };
    let whole = matches
        .get_one::<String>(BODY)
        .map(|text| request::body_value(text))
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

    let has_fields = !fields.is_empty() || upload.is_some();
    let value = match whole {
        None if !has_fields && !body.required => return Ok(None),
        None => Value::Object(fields),
        Some(Value::Object(mut whole)) => {
            whole.extend(fields);
            Value::Object(whole)
        }
        Some(_) if has_fields => {
            return Err(Failure::Usage(
                "--body must be a JSON object when field options are given too.".to_string(),
            ));
        }
        Some(whole) => whole,
    };
    Ok(Some(value))
}

/// A string as it is; any other value as JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
