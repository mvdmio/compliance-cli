use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::failure::{ErrorCode, Failure};

const FILE_NAME: &str = "credentials.json";
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(20);

/// One host's stored sign-in: an Agent connection's tokens and who signed in.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub access_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    pub refresh_token: Option<String>,
    pub user: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Credentials {
    hosts: BTreeMap<String, SignIn>,
}

/// `COMPLIANCE_CONFIG_DIR`, else `compliance/` in the user's config folder.
pub fn folder() -> Result<PathBuf, Failure> {
    match env::var_os("COMPLIANCE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => dirs::config_dir()
            .map(|dir| dir.join("compliance"))
            .ok_or_else(|| {
                Failure::local(
                    ErrorCode::Config,
                    "Found no config folder for the stored sign-in. Set COMPLIANCE_CONFIG_DIR.",
                )
            }),
    }
}

pub fn load(host: &str) -> Result<Option<SignIn>, Failure> {
    Ok(read(&path()?)?.hosts.remove(host))
}

pub fn save(host: &str, sign_in: &SignIn) -> Result<(), Failure> {
    update(|credentials| {
        credentials.hosts.insert(host.to_string(), sign_in.clone());
    })
}

/// Removes the host's entry, and the file once it holds none.
pub fn remove(host: &str) -> Result<(), Failure> {
    update(|credentials| {
        credentials.hosts.remove(host);
    })
}

/// Removes the host's entry when it still holds `refresh_token`. When another process has meanwhile stored a
/// newer sign-in, that one stays and is returned.
pub fn remove_ended(host: &str, refresh_token: &str) -> Result<Option<SignIn>, Failure> {
    update(|credentials| {
        let current = credentials.hosts.get(host)?;
        if current.refresh_token.as_deref() == Some(refresh_token) {
            credentials.hosts.remove(host);
            return None;
        }
        Some(current.clone())
    })
}

/// Unix seconds, the unit of `SignIn::expires_at`.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

impl SignIn {
    pub fn expires_within(&self, seconds: u64) -> bool {
        self.expires_at <= now() + seconds
    }
}

fn path() -> Result<PathBuf, Failure> {
    Ok(folder()?.join(FILE_NAME))
}

/// Reads, changes, and writes the file under a lock, so parallel commands do not undo each other's change.
fn update<T>(change: impl FnOnce(&mut Credentials) -> T) -> Result<T, Failure> {
    let path = path()?;
    let folder = path.parent().expect("the file sits in a folder");
    create_folder(folder).map_err(|error| Failure::file(folder, error))?;
    let _lock = Lock::take(&folder.join(format!("{FILE_NAME}.lock")))?;

    let mut credentials = read(&path)?;
    let answer = change(&mut credentials);
    if credentials.hosts.is_empty() {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Failure::file(&path, error)),
        }
    } else {
        write(&path, &credentials)?;
    }
    Ok(answer)
}

/// A lock file, removed on drop. One left behind by a crashed command is taken over after `LOCK_WAIT`.
struct Lock(PathBuf);

impl Lock {
    fn take(path: &Path) -> Result<Lock, Failure> {
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(_) => return Ok(Lock(path.to_path_buf())),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        // Ignored: another command may have removed it first; the next open decides.
                        let _ = fs::remove_file(path);
                    } else {
                        thread::sleep(LOCK_POLL);
                    }
                }
                Err(error) => return Err(Failure::file(path, error)),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Ignored: a lock file left behind is taken over by the next command.
        let _ = fs::remove_file(&self.0);
    }
}

fn read(path: &Path) -> Result<Credentials, Failure> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Credentials::default());
        }
        Err(error) => return Err(Failure::file(path, error)),
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        Failure::local(
            ErrorCode::Credentials,
            format!(
                "{} does not parse ({error}). Delete it and run `compliance login`.",
                path.display()
            ),
        )
    })
}

/// Writes a temporary file readable only by its owner, then renames it over the old one, so a crash never
/// leaves half a file.
fn write(path: &Path, credentials: &Credentials) -> Result<(), Failure> {
    let folder = path.parent().expect("the file sits in a folder");
    let temporary = folder.join(format!("{FILE_NAME}.{}.tmp", process::id()));
    let bytes = serde_json::to_vec_pretty(credentials).expect("credentials always serialize");
    let written = write_private(&temporary, &bytes).and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = written {
        // Ignored: the temporary file may not exist, and the write error is the one to report.
        let _ = fs::remove_file(&temporary);
        return Err(Failure::file(path, error));
    }
    Ok(())
}

#[cfg(unix)]
fn create_folder(folder: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

#[cfg(not(unix))]
fn create_folder(folder: &Path) -> io::Result<()> {
    fs::create_dir_all(folder)
}

/// A fresh file (`create_new`), so neither a leftover file's mode nor a planted symlink carries over.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
