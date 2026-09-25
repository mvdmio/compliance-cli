//! The API's OpenAPI description, cached per host in the user's cache folder.

use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

use crate::failure::Failure;
use crate::http;

const PATH: &str = "/openapi/v1.json";
const MAX_AGE: Duration = Duration::from_secs(60 * 60);
/// Keeps a host readable in its cache file name, and makes that name unique per host.
const FILE_NAME: &AsciiSet = &NON_ALPHANUMERIC.remove(b'.').remove(b'-');

pub struct Description {
    pub document: Value,
    /// A fetch was tried during this run, so a refetch would bring nothing newer.
    fetch_tried: bool,
}

/// The cached copy when it is younger than `MAX_AGE`, else a fresh one. When the fetch fails, a stale copy is
/// used; with no copy at all the failure is `network`.
pub fn load(host: &str) -> Result<Description, Failure> {
    let path = cache_file(host);
    let cached = path.as_ref().and_then(|path| {
        let document = read_json(fs::read(path).ok()?.as_slice())?;
        let modified = fs::metadata(path).ok()?.modified().ok()?;
        // A modified time in the future counts as fresh.
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();
        Some((document, age))
    });
    if let Some((document, age)) = &cached
        && *age < MAX_AGE
    {
        return Ok(Description {
            document: document.clone(),
            fetch_tried: false,
        });
    }
    match fetch(host) {
        Ok(document) => Ok(Description {
            document,
            fetch_tried: true,
        }),
        Err(failure) => match cached {
            Some((document, _)) => Ok(Description {
                document,
                fetch_tried: true,
            }),
            None => Err(failure),
        },
    }
}

impl Description {
    /// Fetches a fresh copy, once per run. A failed fetch keeps the copy in hand.
    pub fn refetch(&mut self, host: &str) {
        if self.fetch_tried {
            return;
        }
        if let Ok(document) = fetch(host) {
            self.document = document;
        }
        self.fetch_tried = true;
    }
}

/// `COMPLIANCE_CACHE_DIR`, else `compliance/` in the user's cache folder; one file per host.
fn cache_file(host: &str) -> Option<PathBuf> {
    let folder = match env::var_os("COMPLIANCE_CACHE_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs::cache_dir()?.join("compliance"),
    };
    Some(folder.join(format!(
        "openapi-{}.json",
        utf8_percent_encode(host, FILE_NAME)
    )))
}

/// GETs the description without a credential, and caches it.
fn fetch(host: &str) -> Result<Value, Failure> {
    let url = format!("{host}{PATH}");
    let response = http::agent()
        .get(&url)
        .call()
        .map_err(|error| http::transport_failure(&url, error))?;
    let status = response.status().as_u16();
    let unusable = |why: String| {
        Failure::local(
            "network",
            format!("Could not read the API description at {url}: {why}"),
        )
    };
    if status != 200 {
        return Err(unusable(format!("the server answered {status}.")));
    }
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| unusable(error.to_string()))?;
    let document = read_json(&bytes)
        .ok_or_else(|| unusable("it is not an OpenAPI description.".to_string()))?;
    if let Some(path) = cache_file(host) {
        // Ignored: without a cache the next run fetches again.
        let _ = save(&path, &bytes);
    }
    Ok(document)
}

fn read_json(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .filter(|document| document["paths"].is_object())
}

/// Writes through a temporary file, so a parallel run never reads half a copy.
fn save(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}
