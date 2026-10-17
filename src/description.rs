//! The API's OpenAPI description, cached per host in the user's cache folder.

use serde_json::Value;

use crate::cache::{self, Cached};
use crate::failure::{ErrorCode, Failure};
use crate::http;

const PATH: &str = "/openapi/v1.json";
const CACHE_KIND: &str = "openapi";

/// The cached copy when it is younger than an hour, else a fresh one. When the fetch fails, a stale copy is
/// used; with no copy at all the failure is `network`.
pub fn load(host: &str) -> Result<Cached<Value>, Failure> {
    Cached::load(cache::file(CACHE_KIND, host), read_json, || fetch(host))
}

/// GETs the description without a credential, and caches it.
pub fn fetch(host: &str) -> Result<Value, Failure> {
    let url = format!("{host}{PATH}");
    let (status, bytes) = http::read_answer(&url, http::agent().get(&url).call())?;
    let unusable = |why: String| {
        Failure::local(
            ErrorCode::Network,
            format!("Could not read the API description at {url}: {why}"),
        )
    };
    if status != 200 {
        return Err(unusable(format!(
            "the server answered {}.",
            status.as_u16()
        )));
    }
    let document = read_json(&bytes)
        .ok_or_else(|| unusable("it is not an OpenAPI description.".to_string()))?;
    if let Some(path) = cache::file(CACHE_KIND, host) {
        // Ignored: without a cache the next run fetches again.
        let _ = cache::save(&path, &bytes);
    }
    Ok(document)
}

fn read_json(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .filter(|document| document["paths"].is_object())
}
