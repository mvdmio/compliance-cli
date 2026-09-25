use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use serde_json::{Value, json};
use ureq::http::{Response, header};

use crate::failure::Failure;
use crate::output;

const BROWSER_HANDOFF: &str = "browser-handoff";

/// Turns an API answer into the CLI's output. A success prints its JSON body, `{"ok":true,"status":…}` when
/// the body is empty, or, with `out`, writes the body there and prints a summary. A non-JSON body without
/// `out` is refused, so no bytes reach stdout.
pub fn print_response(response: Response<ureq::Body>, out: Option<&Path>) -> Result<(), Failure> {
    let status = response.status().as_u16();
    let content_type = header_text(&response, header::CONTENT_TYPE);
    let file_name = header_text(&response, header::CONTENT_DISPOSITION)
        .as_deref()
        .and_then(content_disposition_file_name);
    let mut reader = response.into_body().into_reader();

    if status >= 400 {
        return Err(problem(
            status,
            content_type.as_deref(),
            read_all(&mut reader)?,
        ));
    }

    if let Some(path) = out {
        let bytes = write_file(&mut reader, path)?;
        output::print_json(&json!({
            "path": path.display().to_string(),
            "bytes": bytes,
            "contentType": content_type,
            "fileName": file_name,
        }));
        return Ok(());
    }

    if !content_type.as_deref().is_some_and(is_json) {
        let mut first = [0u8; 1];
        if reader.read(&mut first).map_err(network)? == 0 {
            return print_empty_success(status);
        }
        return Err(Failure::local(
            "binary-response",
            format!(
                "The response is {}, not JSON. Pass --out <path> to save it.",
                content_type.as_deref().unwrap_or("of no stated type")
            ),
        ));
    }

    let bytes = read_all(&mut reader)?;
    if bytes.is_empty() {
        return print_empty_success(status);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(invalid_json)?;
    output::print_json(&value);
    Ok(())
}

/// The JSON body of a success, or the failure `print_response` would report for an error.
pub fn read_json(response: Response<ureq::Body>) -> Result<Value, Failure> {
    let status = response.status().as_u16();
    let content_type = header_text(&response, header::CONTENT_TYPE);
    let bytes = read_all(&mut response.into_body().into_reader())?;
    if status >= 400 {
        return Err(problem(status, content_type.as_deref(), bytes));
    }
    serde_json::from_slice(&bytes).map_err(invalid_json)
}

fn invalid_json(error: serde_json::Error) -> Failure {
    Failure::local(
        "invalid-response",
        format!("The response claims JSON but does not parse: {error}"),
    )
}

fn print_empty_success(status: u16) -> Result<(), Failure> {
    output::print_json(&json!({ "ok": true, "status": status }));
    Ok(())
}

/// A 403 `browser-handoff` becomes a Browser handoff; any other error body passes through as it came.
fn problem(status: u16, content_type: Option<&str>, body: Vec<u8>) -> Failure {
    let Some(problem) = content_type
        .filter(|content_type| is_json(content_type))
        .and_then(|_| serde_json::from_slice::<Value>(&body).ok())
    else {
        // An empty or non-JSON body (such as a proxy's HTML page) is not the API's; stderr stays JSON.
        return Failure::local(
            "http",
            format!(
                "The server answered {status} with {}.",
                if body.is_empty() {
                    "no body"
                } else {
                    "a body that is not JSON"
                }
            ),
        );
    };
    if status == 403 && problem["type"] == BROWSER_HANDOFF {
        let text = |field: &str| {
            problem[field]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        return Failure::BrowserHandoff {
            reason: text("detail").or_else(|| text("title")),
            handoff_url: text("handoffUrl"),
        };
    }
    Failure::Problem(body)
}

fn header_text(response: &Response<ureq::Body>, name: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn is_json(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
}

/// The file name from `Content-Disposition` (RFC 6266): `filename*` when present, else `filename`.
fn content_disposition_file_name(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';').map(str::trim) {
        let Some((key, raw)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                let extended = raw.trim().splitn(3, '\'').nth(2).and_then(percent_decode);
                if extended.is_some() {
                    return extended;
                }
            }
            "filename" => plain = Some(raw.trim().trim_matches('"').to_string()),
            _ => {}
        }
    }
    plain
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn read_all(reader: &mut impl Read) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(network)?;
    Ok(bytes)
}

fn write_file(reader: &mut impl Read, path: &Path) -> Result<u64, Failure> {
    let file_failure =
        |error: io::Error| Failure::local("file", format!("{}: {error}", path.display()));
    let mut file = File::create(path).map_err(file_failure)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(network(error)),
        };
        file.write_all(&buffer[..read]).map_err(file_failure)?;
        total += read as u64;
    }
}

fn network(error: io::Error) -> Failure {
    Failure::local("network", error.to_string())
}
