//! Pieces of a request that several commands build: URL encoding and the `--body` option.

use std::fs;
use std::path::Path;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

use crate::failure::Failure;

/// Everything but the RFC 3986 unreserved characters is percent-encoded.
pub const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// `text` as one path segment.
pub fn path_segment(text: &str) -> String {
    utf8_percent_encode(text, UNRESERVED).to_string()
}

/// `--body` as bytes: inline JSON, or `@path` to read it from a file. It must parse, so a typo fails before the
/// call.
pub fn body_bytes(text: &str) -> Result<Vec<u8>, Failure> {
    let bytes = body_text(text)?;
    parse_body(&bytes)?;
    Ok(bytes)
}

/// `--body`, parsed.
pub fn body_value(text: &str) -> Result<Value, Failure> {
    parse_body(&body_text(text)?)
}

fn body_text(text: &str) -> Result<Vec<u8>, Failure> {
    match text.strip_prefix('@') {
        Some(path) => fs::read(path).map_err(|error| Failure::file(Path::new(path), error)),
        None => Ok(text.as_bytes().to_vec()),
    }
}

fn parse_body(bytes: &[u8]) -> Result<Value, Failure> {
    serde_json::from_slice(bytes)
        .map_err(|error| Failure::Usage(format!("--body is not valid JSON: {error}")))
}
