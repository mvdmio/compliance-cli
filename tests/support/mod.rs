//! Drives the built `compliance` binary against in-process fake Compliance and Auth hosts.

#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};

/// One request as it reached the fake server.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The path without its query string.
    pub fn path(&self) -> &str {
        self.url.split('?').next().unwrap_or_default()
    }

    /// One field of a form body, or of the query string.
    pub fn form(&self, name: &str) -> Option<String> {
        let body = std::str::from_utf8(&self.body).unwrap_or_default();
        let query = self
            .url
            .split_once('?')
            .map(|(_, query)| query)
            .unwrap_or("");
        form_value(body, name).or_else(|| form_value(query, name))
    }

    /// `http://<Host>`, so a fake can hand out absolute links to itself.
    pub fn origin(&self) -> String {
        format!("http://{}", self.header("Host").expect("a Host header"))
    }
}

pub fn form_value(encoded: &str, name: &str) -> Option<String> {
    encoded.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| {
            percent_decode_str(&value.replace('+', " "))
                .decode_utf8_lossy()
                .into_owned()
        })
    })
}

/// The fake server's answer to one request.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(status: u16, body: &str) -> Self {
        Reply::with_type(status, "application/json; charset=utf-8", body.as_bytes())
    }

    pub fn problem(status: u16, body: &str) -> Self {
        Reply::with_type(status, "application/problem+json", body.as_bytes())
    }

    pub fn with_type(status: u16, content_type: &str, body: &[u8]) -> Self {
        Reply {
            status,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: body.to_vec(),
        }
    }

    pub fn empty(status: u16) -> Self {
        Reply {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

type Handler = dyn Fn(&Recorded) -> Reply + Send + Sync;

/// A fake Compliance host on `127.0.0.1:<free port>` that records every request.
pub struct FakeServer {
    server: Arc<tiny_http::Server>,
    requests: Arc<Mutex<Vec<Recorded>>>,
    worker: Option<JoinHandle<()>>,
}

impl FakeServer {
    pub fn start(handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let server =
            Arc::new(tiny_http::Server::http("127.0.0.1:0").expect("bind the fake server"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Box<Handler> = Box::new(handler);
        let worker = {
            let server = Arc::clone(&server);
            let requests = Arc::clone(&requests);
            thread::spawn(move || {
                for mut request in server.incoming_requests() {
                    let mut body = Vec::new();
                    request
                        .as_reader()
                        .read_to_end(&mut body)
                        .expect("read the request body");
                    let recorded = Recorded {
                        method: request.method().to_string(),
                        url: request.url().to_string(),
                        headers: request
                            .headers()
                            .iter()
                            .map(|header| (header.field.to_string(), header.value.to_string()))
                            .collect(),
                        body,
                    };
                    let reply = handler(&recorded);
                    requests.lock().unwrap().push(recorded);
                    let mut response =
                        tiny_http::Response::from_data(reply.body).with_status_code(reply.status);
                    for (name, value) in reply.headers {
                        response.add_header(
                            tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes())
                                .expect("a valid header"),
                        );
                    }
                    // Ignored: the CLI may hang up early, which the test itself then reports.
                    let _ = request.respond(response);
                }
            })
        };
        FakeServer {
            server,
            requests,
            worker: Some(worker),
        }
    }

    pub fn url(&self) -> String {
        format!(
            "http://{}",
            self.server.server_addr().to_ip().expect("an IP address")
        )
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn only_request(&self) -> Recorded {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// What one run of the binary left behind.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn stdout_json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {self:#?}"))
    }

    pub fn stderr_json(&self) -> Value {
        serde_json::from_str(&self.stderr)
            .unwrap_or_else(|error| panic!("stderr is not JSON ({error}): {self:#?}"))
    }
}

/// Runs `compliance <args>` with only the given `COMPLIANCE_*` variables set.
pub fn compliance(args: &[&str], env: &[(&str, &str)]) -> Run {
    compliance_in(Path::new("."), args, env)
}

pub fn compliance_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let config = tempfile::tempdir().expect("a temporary config folder");
    let output = command(dir, args, env, config.path())
        .output()
        .expect("run the compliance binary");
    Run {
        code: output.status.code().expect("an exit code"),
        stdout: String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    }
}

/// The binary with no display, so `login` never opens a real browser, and with `config` as the config folder
/// and `config/cache` as the cache folder unless `env` names them, so the stored sign-in and cached API
/// description of the person running the tests stay out.
pub fn command(dir: &Path, args: &[&str], env: &[(&str, &str)], config: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_compliance"));
    command.current_dir(dir).args(args);
    for (name, _) in std::env::vars() {
        if name.starts_with("COMPLIANCE_") {
            command.env_remove(name);
        }
    }
    for name in ["DISPLAY", "WAYLAND_DISPLAY"] {
        command.env_remove(name);
    }
    command.env("COMPLIANCE_CONFIG_DIR", config);
    command.env("COMPLIANCE_CACHE_DIR", config.join("cache"));
    command.envs(env.iter().copied());
    command
}

pub const DESCRIPTION_PATH: &str = "/openapi/v1.json";

/// The fixture API description in `tests/fixtures/openapi.json`.
pub fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/openapi.json")).expect("the fixture is JSON")
}

pub const RESOURCE_PATH: &str = "/api";

/// Compliance's Protected Resource Metadata, naming `auth` as the authorization server.
pub fn resource_metadata(request: &Recorded, auth: &str) -> Option<Reply> {
    (request.path() == "/.well-known/oauth-protected-resource/api").then(|| {
        let body = json!({
            "resource": format!("{}{RESOURCE_PATH}", request.origin()),
            "authorization_servers": [auth],
            "bearer_methods_supported": ["header"],
        });
        Reply::json(200, &body.to_string())
    })
}

/// Auth's discovery document, with its endpoints under `/connect/`.
pub fn auth_metadata(request: &Recorded) -> Option<Reply> {
    (request.path() == "/.well-known/openid-configuration").then(|| {
        let origin = request.origin();
        let body = json!({
            "issuer": format!("{origin}/"),
            "authorization_endpoint": format!("{origin}/connect/authorize"),
            "token_endpoint": format!("{origin}/connect/token"),
            "device_authorization_endpoint": format!("{origin}/connect/device"),
            "revocation_endpoint": format!("{origin}/connect/revoke"),
        });
        Reply::json(200, &body.to_string())
    })
}

pub fn oauth_error(error: &str) -> Reply {
    Reply::json(400, &json!({ "error": error }).to_string())
}

pub fn tokens(access_token: &str, refresh_token: &str, id_token: Option<&str>) -> Reply {
    let mut body = json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": 3600,
        "refresh_token": refresh_token,
    });
    if let Some(id_token) = id_token {
        body["id_token"] = json!(id_token);
    }
    Reply::json(200, &body.to_string())
}

/// An unsigned JWT with the User's name and email, as the CLI reads only the claims.
pub fn id_token(name: &str, email: &str) -> String {
    let part = |value: Value| URL_SAFE_NO_PAD.encode(value.to_string());
    format!(
        "{}.{}.signature",
        part(json!({ "alg": "RS256" })),
        part(json!({ "sub": "7", "name": name, "email": email }))
    )
}

pub fn accounts_list() -> Reply {
    let body = json!({
        "account": { "id": 2, "name": "Beta" },
        "items": [
            { "id": 1, "name": "Alpha", "current": false },
            { "id": 2, "name": "Beta", "current": true },
        ],
        "total": 2,
        "nextOffset": null,
    });
    Reply::json(200, &body.to_string())
}

pub fn credentials_path(config: &Path) -> std::path::PathBuf {
    config.join("credentials.json")
}

/// Stores a sign-in for `host` as `compliance login` leaves it.
pub fn store_sign_in(
    config: &Path,
    host: &str,
    access_token: &str,
    expires_at: u64,
    refresh_token: &str,
) {
    let path = credentials_path(config);
    let mut file = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or_else(|| json!({ "hosts": {} }));
    file["hosts"][host] = json!({
        "accessToken": access_token,
        "expiresAt": expires_at,
        "refreshToken": refresh_token,
        "user": { "name": "Ada Lovelace", "email": "ada@example.test" },
    });
    fs::write(path, file.to_string()).expect("write the credentials file");
}

pub fn read_credentials(config: &Path) -> Value {
    serde_json::from_slice(&fs::read(credentials_path(config)).expect("read the credentials file"))
        .expect("the credentials file is JSON")
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs()
}
