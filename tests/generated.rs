mod support;

use std::fs::{self, File};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use support::{
    DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, TOKEN, body_json, compliance_in, fixture,
};
use tempfile::TempDir;

const FILE_BYTES: &[u8] = b"\x00\x01binary\xff";

/// A fake Compliance host that serves an API description which a test can change between runs, and one cache
/// folder shared by every run of the test.
struct Host {
    server: FakeServer,
    description: Arc<Mutex<Option<Value>>>,
    cache: TempDir,
}

impl Host {
    fn start() -> Self {
        let description = Arc::new(Mutex::new(Some(fixture())));
        let served = Arc::clone(&description);
        let server = FakeServer::start(move |request| {
            if request.path() == DESCRIPTION_PATH {
                return match served.lock().unwrap().as_ref() {
                    Some(document) => Reply::json(200, &document.to_string()),
                    None => Reply::empty(503),
                };
            }
            if request.path().ends_with("/file") {
                return Reply::with_type(200, "application/octet-stream", FILE_BYTES)
                    .header("Content-Disposition", "attachment; filename=\"scan.pdf\"");
            }
            Reply::json(200, "{\"done\":true}")
        });
        Host {
            server,
            description,
            cache: tempfile::tempdir().expect("a cache folder"),
        }
    }

    fn serve(&self, document: Option<Value>) {
        *self.description.lock().unwrap() = document;
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_in(&std::env::temp_dir(), args)
    }

    fn run_in(&self, dir: &std::path::Path, args: &[&str]) -> Run {
        let url = self.server.url();
        let cache = self.cache.path().to_str().expect("a UTF-8 path");
        compliance_in(
            dir,
            args,
            &[
                ("COMPLIANCE_URL", &url),
                ("COMPLIANCE_TOKEN", TOKEN),
                ("COMPLIANCE_CACHE_DIR", cache),
            ],
        )
    }

    fn description_fetches(&self) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|request| request.path() == DESCRIPTION_PATH)
            .count()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .collect()
    }

    fn only_api_request(&self) -> Recorded {
        let requests = self.api_requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }

    /// Makes every cached copy look `age` old.
    fn age_cache(&self, age: Duration) {
        for entry in fs::read_dir(self.cache.path()).expect("the cache folder") {
            let file = File::options()
                .write(true)
                .open(entry.expect("a cache entry").path())
                .expect("open the cached copy");
            file.set_modified(SystemTime::now() - age)
                .expect("age the cached copy");
        }
    }
}

fn with_operation(group_action: &str, path: &str) -> Value {
    let mut document = fixture();
    document["paths"][path] = json!({
        "post": {
            "tags": ["Risks"],
            "summary": "A new operation",
            "operationId": group_action,
            "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

#[test]
fn an_action_with_a_path_parameter_sends_its_method_and_path() {
    let host = Host::start();

    let run = host.run(&["risks", "accept", "r-1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "done": true }));
    let request = host.only_api_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "/api/v1/risks/r-1/accept");
    assert_eq!(request.header("Authorization"), Some("Bearer cmp_pat_test"));
    assert!(request.body.is_empty());
}

#[test]
fn path_parameters_follow_the_path_order_and_are_encoded() {
    let host = Host::start();

    let run = host.run(&["frameworks", "get-requirement", "iso 27001", "A.5/1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        host.only_api_request().url,
        "/api/v1/frameworks/iso%2027001/requirements/A.5%2F1"
    );
}

#[test]
fn odata_options_send_their_dollar_names() {
    let host = Host::start();

    let run = host.run(&[
        "risks",
        "list",
        "--filter",
        "status eq 'Open'",
        "--top",
        "5",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.path(), "/api/v1/risks");
    assert_eq!(request.form("$filter").as_deref(), Some("status eq 'Open'"));
    assert_eq!(request.form("$top").as_deref(), Some("5"));
    assert!(request.url.contains("$top=5"), "{}", request.url);
}

#[test]
fn an_integer_option_refuses_text_before_any_request() {
    let host = Host::start();

    let run = host.run(&["risks", "list", "--top", "five"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn body_options_are_typed_from_the_schema() {
    let host = Host::start();

    let run = host.run(&[
        "risks",
        "create",
        "--title",
        "Flood",
        "--status",
        "Open",
        "--likelihood",
        "3",
        "--score",
        "-2.5",
        "--private",
        "true",
        "--owner-ids",
        "a1",
        "--owner-ids",
        "b2",
        "--collected-at",
        "2026-01-02",
        "--owner",
        "{\"kind\":\"Person\",\"id\":\"p1\"}",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(
        body_json(&request),
        json!({
            "title": "Flood",
            "status": "Open",
            "likelihood": 3,
            "score": -2.5,
            "private": true,
            "ownerIds": ["a1", "b2"],
            "collectedAt": "2026-01-02",
            "owner": { "kind": "Person", "id": "p1" },
        })
    );
}

#[test]
fn field_options_win_over_the_same_keys_in_body() {
    let host = Host::start();

    let run = host.run(&[
        "risks",
        "create",
        "--body",
        "{\"title\":\"Old\",\"likelihood\":1}",
        "--title",
        "New",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "title": "New", "likelihood": 1 })
    );
}

#[test]
fn body_reads_a_file() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("risk.json"), "{\"title\":\"From a file\"}").unwrap();

    let run = host.run_in(dir.path(), &["risks", "create", "--body", "@risk.json"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "title": "From a file" })
    );
}

#[test]
fn a_required_body_with_no_input_is_an_empty_object_and_an_optional_one_is_left_out() {
    let host = Host::start();

    let required = host.run(&["risks", "create"]);
    let optional = host.run(&["risks", "update", "r1"]);

    assert_eq!(required.code, 0, "{required:#?}");
    assert_eq!(optional.code, 0, "{optional:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].body, b"{}");
    assert_eq!(requests[1].method, "PATCH");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[1].header("Content-Type"), None);
}

#[test]
fn an_enum_option_refuses_other_values_and_lists_them_in_help() {
    let host = Host::start();

    let wrong = host.run(&["risks", "create", "--status", "Maybe"]);
    let help = host.run(&["risks", "create", "--help"]);

    assert_eq!(wrong.code, 2, "{wrong:#?}");
    assert!(host.api_requests().is_empty());
    assert!(help.stdout.contains("Open"), "{}", help.stdout);
    assert!(help.stdout.contains("Accepted"), "{}", help.stdout);
    assert!(help.stdout.contains("Closed"), "{}", help.stdout);
}

#[test]
fn a_clash_renames_the_later_option_and_help_says_so() {
    let host = Host::start();

    let run = host.run(&[
        "risks",
        "update",
        "r1",
        "--notify",
        "true",
        "--field-notify",
        "false",
        "--field-out",
        "Transfer",
    ]);
    let help = host.run(&["risks", "update", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.api_requests().remove(0);
    assert_eq!(request.path(), "/api/v1/risks/r1");
    assert_eq!(request.form("notify").as_deref(), Some("true"));
    assert_eq!(
        body_json(&request),
        json!({ "notify": false, "out": "Transfer" })
    );
    assert!(
        help.stdout
            .contains("named --field-notify because --notify is taken"),
        "{}",
        help.stdout
    );
    assert!(
        help.stdout
            .contains("named --field-out because --out is taken"),
        "{}",
        help.stdout
    );
}

#[test]
fn help_on_an_action_shows_its_summary_description_and_parameters() {
    let host = Host::start();

    let accept = host.run(&["risks", "accept", "--help"]);
    let create = host.run(&["risks", "create", "--help"]);
    let list = host.run(&["risks", "list", "--help"]);

    assert_eq!(accept.code, 0, "{accept:#?}");
    for expected in [
        "Accept a Risk",
        "Accepts the Risk as it stands, so it needs no treatment.",
        "The Risk id.",
        "Type: string.",
    ] {
        assert!(
            accept.stdout.contains(expected),
            "{expected}: {}",
            accept.stdout
        );
    }
    for expected in [
        "Create a Risk",
        "A short name for the Risk.",
        "--likelihood <INTEGER>",
        "--score <NUMBER>",
        "--owner <JSON>",
        "--body <JSON|@FILE>",
        "Required in the body.",
    ] {
        assert!(
            create.stdout.contains(expected),
            "{expected}: {}",
            create.stdout
        );
    }
    assert!(list.stdout.contains("An OData filter."), "{}", list.stdout);
    assert!(list.stdout.contains("--top <INTEGER>"), "{}", list.stdout);
    assert!(host.api_requests().is_empty());
}

#[test]
fn help_on_a_group_lists_its_actions_and_its_tags() {
    let host = Host::start();

    let run = host.run(&["frameworks", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "list-proposed-coverage",
        "List proposed coverage",
        "get-requirement",
        "Frameworks, Proposed coverage",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn help_lists_the_hand_written_commands_and_the_generated_groups() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "login",
        "status",
        "api",
        "accounts",
        "risks",
        "frameworks",
        "evidence",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn a_second_run_within_the_hour_uses_the_cached_copy() {
    let host = Host::start();

    host.run(&["risks", "accept", "r1"]);
    let second = host.run(&["risks", "accept", "r2"]);

    assert_eq!(second.code, 0, "{second:#?}");
    assert_eq!(host.description_fetches(), 1);
}

#[test]
fn a_copy_older_than_an_hour_is_fetched_again() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(61 * 60));

    let run = host.run(&["risks", "accept", "r2"]);
    let after = host.run(&["risks", "accept", "r3"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(after.code, 0, "{after:#?}");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_stale_copy_is_used_when_the_fetch_fails() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["risks", "accept", "r2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/risks/r2/accept");
}

#[test]
fn no_copy_and_no_fetch_is_a_network_error() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["risks", "accept", "r1"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
    assert!(host.api_requests().is_empty());
}

#[test]
fn hand_written_commands_never_fetch_the_description() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["api", "GET", "/api/v1/risks"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn an_operation_added_after_caching_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);
    host.serve(Some(with_operation(
        "risks.archive",
        "/api/v1/risks/{id}/archive",
    )));

    let run = host.run(&["risks", "archive", "r1"]);
    let again = host.run(&["risks", "archive", "r2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(again.code, 0, "{again:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/risks/r1/archive");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_new_group_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);
    host.serve(Some(with_operation(
        "suppliers.terminate",
        "/api/v1/suppliers/{id}/terminate",
    )));

    let run = host.run(&["suppliers", "terminate", "s1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/suppliers/s1/terminate");
}

#[test]
fn a_command_still_unknown_after_the_refetch_is_a_usage_mistake() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);

    let action = host.run(&["risks", "explode", "r1"]);
    let group = host.run(&["dragons", "list"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(action.stderr_json()["error"], "usage");
    assert_eq!(group.code, 2, "{group:#?}");
    assert_eq!(host.description_fetches(), 3);
    assert_eq!(host.api_requests().len(), 1);
}

#[test]
fn the_hand_written_accounts_list_wins_over_the_generated_one() {
    let host = Host::start();

    // The generated accounts.list takes --top; the hand-written one does not.
    let hand_written = host.run(&["accounts", "list", "--top", "1"]);
    let generated = host.run(&["accounts", "get", "7"]);
    let help = host.run(&["accounts", "--help"]);

    assert_eq!(hand_written.code, 2, "{hand_written:#?}");
    assert_eq!(generated.code, 0, "{generated:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/accounts/7");
    for expected in ["list", "switch", "get", "Get an Account"] {
        assert!(
            help.stdout.contains(expected),
            "{expected}: {}",
            help.stdout
        );
    }
}

#[test]
fn a_download_writes_the_bytes_to_out_and_prints_a_summary() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();

    let run = host.run_in(
        dir.path(),
        &["evidence", "download", "e1", "--out", "scan.pdf"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(fs::read(dir.path().join("scan.pdf")).unwrap(), FILE_BYTES);
    assert_eq!(
        run.stdout_json(),
        json!({
            "path": "scan.pdf",
            "bytes": FILE_BYTES.len(),
            "contentType": "application/octet-stream",
            "fileName": "scan.pdf",
        })
    );
    assert_eq!(host.only_api_request().url, "/api/v1/evidence/e1/file");
}

#[test]
fn a_download_without_out_is_a_usage_mistake_and_sends_no_request() {
    let host = Host::start();

    let run = host.run(&["evidence", "download", "e1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn out_on_a_command_that_answers_json_is_a_usage_mistake() {
    let host = Host::start();

    let run = host.run(&["risks", "accept", "r1", "--out", "risk.json"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn an_upload_target_field_says_so_in_help() {
    let host = Host::start();

    let run = host.run(&["evidence", "create", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(
        run.stdout.contains("--upload-id <STRING>"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("upload target `evidence`"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_generated_error_answer_keeps_the_exit_codes() {
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(
            403,
            "{\"type\":\"browser-handoff\",\"detail\":\"Confirm in the browser.\",\"handoffUrl\":\"https://example.test/go\"}",
        )
    });

    let run = compliance_in(
        &std::env::temp_dir(),
        &["risks", "accept", "r1"],
        &[
            ("COMPLIANCE_URL", &server.url()),
            ("COMPLIANCE_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(run.stdout_json()["handoffUrl"], "https://example.test/go");
}

#[test]
fn an_integer_enum_is_checked_and_sent_as_a_number() {
    let host = Host::start();

    let wrong = host.run(&["risks", "create", "--impact", "4"]);
    let right = host.run(&["risks", "create", "--impact", "2"]);

    assert_eq!(wrong.code, 2, "{wrong:#?}");
    assert_eq!(right.code, 0, "{right:#?}");
    assert_eq!(body_json(&host.only_api_request()), json!({ "impact": 2 }));
}

#[test]
fn help_without_the_description_lists_the_hand_written_commands_and_says_what_is_missing() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(run.stdout.contains("login"), "{}", run.stdout);
    assert!(!run.stdout.contains("risks"), "{}", run.stdout);
    assert!(
        run.stdout.contains("openapi/v1.json are missing"),
        "{}",
        run.stdout
    );
}

#[test]
fn an_unknown_command_with_a_failed_fetch_does_not_fetch_twice() {
    let host = Host::start();
    host.run(&["risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["risks", "explode", "r1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(host.description_fetches(), 2);
}
