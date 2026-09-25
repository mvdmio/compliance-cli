//! Drives the built `compliance` binary against an in-process fake Compliance host.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde_json::Value;

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
    let mut command = Command::new(env!("CARGO_BIN_EXE_compliance"));
    command.current_dir(dir).args(args);
    for (name, _) in std::env::vars() {
        if name.starts_with("COMPLIANCE_") {
            command.env_remove(name);
        }
    }
    command.envs(env.iter().copied());
    let output = command.output().expect("run the compliance binary");
    Run {
        code: output.status.code().expect("an exit code"),
        stdout: String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    }
}
