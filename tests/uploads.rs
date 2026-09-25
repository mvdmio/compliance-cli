mod support;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use support::{DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, compliance, fixture};
use tempfile::TempDir;

const TOKEN: &str = "cmp_pat_test";
const PART_SIZE: u64 = 4;
const CREATED: &str = "{\"id\":\"e1\",\"title\":\"T\"}";

/// What the fake Upload link does to one `PUT ?offset=N`, until `times` runs out.
#[derive(Clone, Copy)]
enum Fault {
    /// Answers 503 and keeps nothing, as when the part was lost on the way.
    Unavailable,
    /// Keeps the part, then answers 409 with `bytesReceived`, as when the part arrived twice.
    Conflict,
}

struct Link {
    upload_id: String,
    bytes: Vec<u8>,
    finished: bool,
}

#[derive(Default)]
struct Links {
    links: HashMap<String, Link>,
    faults: HashMap<u64, (Fault, u32)>,
    max_bytes: Option<u64>,
}

/// A fake Compliance host that serves the fixture description, `POST /api/v1/uploads`, and the Upload links it
/// hands out under `/uploads/<token>`, with a part size of `PART_SIZE`.
struct Host {
    server: FakeServer,
    links: Arc<Mutex<Links>>,
    files: TempDir,
}

impl Host {
    fn start() -> Self {
        Host::serving(fixture())
    }

    fn serving(description: Value) -> Self {
        let links = Arc::new(Mutex::new(Links::default()));
        let state = Arc::clone(&links);
        let server = FakeServer::start(move |request| {
            if request.path() == DESCRIPTION_PATH {
                return Reply::json(200, &description.to_string());
            }
            let mut links = state.lock().unwrap();
            if request.path() == "/api/v1/uploads" {
                return links.create(request);
            }
            if let Some(rest) = request.path().strip_prefix("/uploads/") {
                return links.link(request, rest);
            }
            Reply::json(201, CREATED)
        });
        Host {
            server,
            links,
            files: tempfile::tempdir().expect("a folder for the files"),
        }
    }

    fn fault(&self, offset: u64, fault: Fault, times: u32) {
        self.links
            .lock()
            .unwrap()
            .faults
            .insert(offset, (fault, times));
    }

    fn max_bytes(&self, max_bytes: u64) {
        self.links.lock().unwrap().max_bytes = Some(max_bytes);
    }

    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path: PathBuf = self.files.path().join(name);
        fs::write(&path, bytes).expect("write the file");
        path.to_str().expect("a UTF-8 path").to_string()
    }

    fn run(&self, args: &[&str]) -> Run {
        let url = self.server.url();
        compliance(
            args,
            &[("COMPLIANCE_URL", &url), ("COMPLIANCE_TOKEN", TOKEN)],
        )
    }

    fn requests(&self) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .collect()
    }

    fn link_requests(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|request| request.path().starts_with("/uploads/"))
            .collect()
    }

    fn received(&self, token: &str) -> Vec<u8> {
        self.links.lock().unwrap().links[token].bytes.clone()
    }

    fn finished(&self, token: &str) -> bool {
        self.links.lock().unwrap().links[token].finished
    }
}

impl Links {
    fn create(&mut self, request: &Recorded) -> Reply {
        let body = body_json(request);
        let number = self.links.len() + 1;
        let token = format!("t{number}");
        let upload_id = format!("00000000-0000-0000-0000-00000000000{number}");
        self.links.insert(
            token.clone(),
            Link {
                upload_id: upload_id.clone(),
                bytes: Vec::new(),
                finished: false,
            },
        );
        let answer = json!({
            "account": { "id": 1, "name": "Alpha" },
            "uploadId": upload_id,
            "url": format!("{}/uploads/{token}", request.origin()),
            "target": body["target"],
            "fileName": body["fileName"],
            "partSize": PART_SIZE,
            "maxBytes": self.max_bytes,
            "expiresAt": "2030-01-01T00:00:00Z",
        });
        Reply::json(201, &answer.to_string())
    }

    fn link(&mut self, request: &Recorded, rest: &str) -> Reply {
        let (token, finish) = match rest.strip_suffix("/finish") {
            Some(token) => (token, true),
            None => (rest, false),
        };
        let offset = request
            .form("offset")
            .map(|offset| offset.parse::<u64>().unwrap());
        let fault = offset.and_then(|offset| {
            let (fault, times) = self.faults.get_mut(&offset)?;
            (*times > 0).then(|| {
                *times -= 1;
                *fault
            })
        });
        let Some(link) = self.links.get_mut(token) else {
            return Reply::problem(404, "{\"status\":404}");
        };
        match (request.method.as_str(), finish, offset) {
            ("GET", false, None) => Reply::json(200, &status(link)),
            ("PUT", false, None) => {
                link.bytes = request.body.clone();
                link.finished = true;
                Reply::json(200, &status(link))
            }
            ("PUT", false, Some(offset)) => {
                if link.finished {
                    return Reply::problem(409, "{\"status\":409}");
                }
                if offset != link.bytes.len() as u64 {
                    return conflict(link);
                }
                assert!(request.body.len() as u64 <= PART_SIZE, "a part too large");
                match fault {
                    Some(Fault::Unavailable) => Reply::empty(503),
                    Some(Fault::Conflict) => {
                        link.bytes.extend_from_slice(&request.body);
                        conflict(link)
                    }
                    None => {
                        link.bytes.extend_from_slice(&request.body);
                        Reply::json(200, &status(link))
                    }
                }
            }
            ("POST", true, None) => {
                let total = body_json(request)["totalBytes"].as_u64();
                if total != Some(link.bytes.len() as u64) {
                    return conflict(link);
                }
                link.finished = true;
                Reply::json(200, &status(link))
            }
            _ => Reply::empty(405),
        }
    }
}

fn status(link: &Link) -> String {
    json!({
        "uploadId": link.upload_id,
        "bytesReceived": link.bytes.len(),
        "finished": link.finished,
        "partSize": PART_SIZE,
    })
    .to_string()
}

fn conflict(link: &Link) -> Reply {
    Reply::problem(
        409,
        &json!({ "status": 409, "bytesReceived": link.bytes.len() }).to_string(),
    )
}

fn body_json(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).expect("a JSON body")
}

/// Each request as `METHOD path?query`.
fn lines(requests: &[Recorded]) -> Vec<String> {
    requests
        .iter()
        .map(|request| format!("{} {}", request.method, request.url))
        .collect()
}

const BIG: &[u8] = b"0123456789";

#[test]
fn a_small_file_goes_in_one_put_and_then_the_call_carries_its_upload_id() {
    let host = Host::start();
    let file = host.file("small.pdf", b"%PDF");

    let run = host.run(&["evidence", "create", "--file", &file, "--title", "T"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        serde_json::from_str::<Value>(CREATED).unwrap()
    );
    let requests = host.requests();
    assert_eq!(
        lines(&requests),
        [
            "POST /api/v1/uploads",
            "PUT /uploads/t1",
            "POST /api/v1/evidence"
        ]
    );
    assert_eq!(
        body_json(&requests[0]),
        json!({ "target": "evidence", "fileName": "small.pdf" })
    );
    assert_eq!(requests[1].body, b"%PDF");
    assert_eq!(
        body_json(&requests[2]),
        json!({ "uploadId": "00000000-0000-0000-0000-000000000001", "title": "T" })
    );
    assert!(host.finished("t1"));
}

#[test]
fn a_file_larger_than_the_part_size_goes_in_parts_and_then_finishes() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 0, "{run:#?}");
    let links = host.link_requests();
    assert_eq!(
        lines(&links),
        [
            "PUT /uploads/t1?offset=0",
            "PUT /uploads/t1?offset=4",
            "PUT /uploads/t1?offset=8",
            "POST /uploads/t1/finish",
        ]
    );
    let sizes: Vec<usize> = links[..3]
        .iter()
        .map(|request| request.body.len())
        .collect();
    assert_eq!(sizes, [4, 4, 2]);
    assert_eq!(
        body_json(&links[3]),
        json!({ "totalBytes": 10, "fileName": "big.zip" })
    );
    assert_eq!(host.received("t1"), BIG);
    assert!(host.finished("t1"));
    let last = host.requests().pop().unwrap();
    assert_eq!(last.path(), "/api/v1/evidence");
    assert_eq!(
        body_json(&last),
        json!({ "uploadId": "00000000-0000-0000-0000-000000000001" })
    );
}

#[test]
fn no_request_to_the_upload_link_carries_the_credential() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);
    host.fault(4, Fault::Unavailable, 1);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 0, "{run:#?}");
    let links = host.link_requests();
    assert!(links.iter().any(|request| request.method == "GET"));
    for request in &links {
        assert_eq!(request.header("Authorization"), None, "{request:#?}");
    }
    let bearer = format!("Bearer {TOKEN}");
    let upload = &host.requests()[0];
    assert_eq!(upload.header("Authorization"), Some(bearer.as_str()));
}

#[test]
fn after_a_409_the_upload_reads_the_link_and_goes_on_from_the_bytes_received() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);
    host.fault(4, Fault::Conflict, 1);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 0, "{run:#?}");
    let links = host.link_requests();
    assert_eq!(
        lines(&links),
        [
            "PUT /uploads/t1?offset=0",
            "PUT /uploads/t1?offset=4",
            "GET /uploads/t1",
            "PUT /uploads/t1?offset=8",
            "POST /uploads/t1/finish",
        ]
    );
    assert_eq!(links[2].header("Accept"), Some("application/json"));
    assert_eq!(host.received("t1"), BIG);
}

#[test]
fn after_a_dropped_part_the_upload_resumes_and_completes() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);
    host.fault(4, Fault::Unavailable, 2);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        lines(&host.link_requests()),
        [
            "PUT /uploads/t1?offset=0",
            "PUT /uploads/t1?offset=4",
            "GET /uploads/t1",
            "PUT /uploads/t1?offset=4",
            "GET /uploads/t1",
            "PUT /uploads/t1?offset=4",
            "PUT /uploads/t1?offset=8",
            "POST /uploads/t1/finish",
        ]
    );
    assert_eq!(host.received("t1"), BIG);
    assert!(host.finished("t1"));
}

#[test]
fn an_upload_gives_up_after_five_tries_in_a_row_without_progress() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);
    host.fault(4, Fault::Unavailable, u32::MAX);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "upload");
    assert_eq!(run.stdout, "");
    let tries = host
        .link_requests()
        .iter()
        .filter(|request| request.url == "/uploads/t1?offset=4")
        .count();
    assert_eq!(tries, 5);
    assert!(
        host.requests()
            .iter()
            .all(|request| request.path() != "/api/v1/evidence")
    );
}

#[test]
fn a_file_larger_than_the_link_takes_is_refused_before_any_put() {
    let host = Host::start();
    let file = host.file("big.zip", BIG);
    host.max_bytes(9);

    let run = host.run(&["evidence", "create", "--file", &file]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "file");
    assert_eq!(lines(&host.requests()), ["POST /api/v1/uploads"]);
}

#[test]
fn a_missing_file_fails_before_any_request() {
    let host = Host::start();
    let missing = host.files.path().join("missing.pdf");

    let run = host.run(&["evidence", "create", "--file", missing.to_str().unwrap()]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "file");
    assert!(host.requests().is_empty());
}

#[test]
fn a_missing_second_file_fails_before_the_first_is_sent() {
    let host = Host::start();
    let first = host.file("a.txt", b"first");
    let missing = host.files.path().join("missing.txt");

    let run = host.run(&[
        "conversations",
        "send-message",
        "c1",
        "--file",
        &first,
        "--file",
        missing.to_str().unwrap(),
    ]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "file");
    assert!(host.requests().is_empty());
}

#[test]
fn a_file_and_an_upload_id_together_are_a_usage_mistake() {
    let host = Host::start();
    let file = host.file("small.pdf", b"%PDF");

    let run = host.run(&["evidence", "create", "--file", &file, "--upload-id", "u1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.requests().is_empty());
}

#[test]
fn a_repeated_file_on_a_list_of_upload_ids_sends_every_id_in_order() {
    let host = Host::start();
    let first = host.file("a.txt", b"first");
    let second = host.file("b.txt", b"two");

    let run = host.run(&[
        "conversations",
        "send-message",
        "c1",
        "--text",
        "hi",
        "--file",
        &first,
        "--file",
        &second,
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let requests = host.requests();
    let uploads: Vec<Value> = requests
        .iter()
        .filter(|request| request.path() == "/api/v1/uploads")
        .map(body_json)
        .collect();
    assert_eq!(
        uploads,
        [
            json!({ "target": "conversation", "fileName": "a.txt" }),
            json!({ "target": "conversation", "fileName": "b.txt" }),
        ]
    );
    assert_eq!(host.received("t1"), b"first");
    assert_eq!(host.received("t2"), b"two");
    let last = requests.last().unwrap();
    assert_eq!(last.path(), "/api/v1/conversations/c1/messages");
    assert_eq!(
        body_json(last),
        json!({
            "text": "hi",
            "uploadIds": [
                "00000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000002",
            ],
        })
    );
}

#[test]
fn an_upload_target_query_parameter_takes_a_file_too() {
    let mut document = fixture();
    document["paths"]["/api/v1/import-sessions"] = json!({
        "post": {
            "tags": ["Imports"],
            "operationId": "imports.create",
            "parameters": [{
                "name": "uploadId",
                "in": "query",
                "required": true,
                "x-upload-target": "import",
                "schema": { "type": "string" }
            }],
            "responses": { "201": { "description": "Created", "content": { "application/json": {} } } }
        }
    });
    let host = Host::serving(document);
    let file = host.file("space.zip", b"PK");

    let run = host.run(&["imports", "create", "--file", &file]);

    assert_eq!(run.code, 0, "{run:#?}");
    let requests = host.requests();
    assert_eq!(body_json(&requests[0])["target"], "import");
    assert_eq!(
        requests.last().unwrap().url,
        "/api/v1/import-sessions?uploadId=00000000-0000-0000-0000-000000000001"
    );
}

#[test]
fn help_shows_the_file_option_with_its_upload_target() {
    let host = Host::start();

    let single = host.run(&["evidence", "create", "--help"]);
    let list = host.run(&["conversations", "send-message", "--help"]);

    assert_eq!(single.code, 0, "{single:#?}");
    assert!(single.stdout.contains("--file <PATH>"), "{}", single.stdout);
    assert!(
        single
            .stdout
            .contains("Upload this file to the upload target `evidence`"),
        "{}",
        single.stdout
    );
    assert!(
        list.stdout.contains("upload target `conversation`")
            && list.stdout.contains("Repeat it for more files"),
        "{}",
        list.stdout
    );
}
