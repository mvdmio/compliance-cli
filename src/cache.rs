use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, SystemTime};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::failure::Failure;

pub const MAX_AGE: Duration = Duration::from_secs(60 * 60);
const FILE_NAME: &AsciiSet = &NON_ALPHANUMERIC.remove(b'.').remove(b'-');

pub fn file(kind: &str, key: &str) -> Option<PathBuf> {
    let folder = match env::var_os("COMPLIANCE_CACHE_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs::cache_dir()?.join("compliance"),
    };
    Some(folder.join(format!(
        "{kind}-{}.json",
        utf8_percent_encode(key, FILE_NAME)
    )))
}

pub fn read<T>(path: &Path, parse: impl FnOnce(&[u8]) -> Option<T>) -> Option<(T, Duration)> {
    let value = parse(fs::read(path).ok()?.as_slice())?;
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some((value, age))
}

pub struct Cached<T> {
    pub value: T,
    fetch_tried: bool,
}

impl<T> Cached<T> {
    pub fn load(
        path: Option<PathBuf>,
        parse: impl FnOnce(&[u8]) -> Option<T>,
        fetch: impl FnOnce() -> Result<T, Failure>,
    ) -> Result<Self, Failure> {
        let stale = match path.and_then(|path| read(&path, parse)) {
            Some((value, age)) if age < MAX_AGE => {
                return Ok(Cached {
                    value,
                    fetch_tried: false,
                });
            }
            other => other.map(|(value, _)| value),
        };
        match fetch() {
            Ok(value) => Ok(Cached {
                value,
                fetch_tried: true,
            }),
            Err(failure) => stale
                .map(|value| Cached {
                    value,
                    fetch_tried: true,
                })
                .ok_or(failure),
        }
    }

    pub fn refetch(&mut self, fetch: impl FnOnce() -> Result<T, Failure>) {
        if self.fetch_tried {
            return;
        }
        if let Ok(value) = fetch() {
            self.value = value;
        }
        self.fetch_tried = true;
    }
}

pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}
