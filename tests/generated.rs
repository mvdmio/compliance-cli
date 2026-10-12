#[path = "e2e/operations.rs"]
mod operations;
mod support;

use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use operations::{CommandName, holds_chat, live_operations};
use serde_json::{Value, json};
use support::{
    DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, TOKEN, body_json, closed_address, fixture,
    mvdm_in, without_operations,
};
use tempfile::TempDir;

const FILE_BYTES: &[u8] = b"\x00\x01binary\xff";

/// A fake Compliance host that serves an API description which a test can change between runs, and one cache
/// folder shared by every run of the test.
struct Host {
    server: FakeServer,
    description: Arc<Mutex<Option<Value>>>,
    cache: TempDir,
    /// An Auth address nothing listens on, so a run never reaches the real one.
    auth: String,
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
            auth: closed_address(),
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
        mvdm_in(
            dir,
            args,
            &[
                ("MVDM_COMPLIANCE_URL", &url),
                ("MVDM_AUTH_URL", &self.auth),
                ("MVDM_TOKEN", TOKEN),
                ("MVDM_CACHE_DIR", cache),
            ],
        )
    }

    fn description_fetches(&self) -> usize {
        self.server.description_fetches()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.server.api_requests()
    }

    fn only_api_request(&self) -> Recorded {
        let requests = self.api_requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }

    /// Makes every cached copy look `age` old.
    fn age_cache(&self, age: Duration) {
        support::age_files(self.cache.path(), age);
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

    let run = host.run(&["compliance", "risks", "accept", "r-1"]);

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

    let run = host.run(&[
        "compliance",
        "frameworks",
        "get-requirement",
        "iso 27001",
        "A.5/1",
    ]);

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
        "compliance",
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

    let run = host.run(&["compliance", "risks", "list", "--top", "five"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn body_options_are_typed_from_the_schema() {
    let host = Host::start();

    let run = host.run(&[
        "compliance",
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
        "compliance",
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

    let run = host.run_in(
        dir.path(),
        &["compliance", "risks", "create", "--body", "@risk.json"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "title": "From a file" })
    );
}

#[test]
fn a_required_body_with_no_input_is_an_empty_object_and_an_optional_one_is_left_out() {
    let host = Host::start();

    let required = host.run(&["compliance", "risks", "create"]);
    let optional = host.run(&["compliance", "risks", "update", "r1"]);

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

    let wrong = host.run(&["compliance", "risks", "create", "--status", "Maybe"]);
    let help = host.run(&["compliance", "risks", "create", "--help"]);

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
        "compliance",
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
    let help = host.run(&["compliance", "risks", "update", "--help"]);

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

    let accept = host.run(&["compliance", "risks", "accept", "--help"]);
    let create = host.run(&["compliance", "risks", "create", "--help"]);
    let list = host.run(&["compliance", "risks", "list", "--help"]);

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

    let run = host.run(&["compliance", "frameworks", "--help"]);

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
fn the_product_help_lists_the_hand_written_commands_and_the_generated_groups() {
    let host = Host::start();

    let run = host.run(&["compliance", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in ["api", "chat", "accounts", "risks", "frameworks", "evidence"] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
    // The top-level commands are not the Product's.
    assert!(!run.stdout.contains("login"), "{}", run.stdout);
    assert!(!run.stdout.contains("logout"), "{}", run.stdout);
}

#[test]
fn the_top_level_help_lists_a_product_a_variable_names_when_the_list_cannot_be_read() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in ["login", "status", "accounts", "compliance"] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
    assert!(!run.stdout.contains("risks"), "{}", run.stdout);
    assert!(
        run.stdout.contains("could not be read from Auth"),
        "{}",
        run.stdout
    );
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn a_second_run_within_the_hour_uses_the_cached_copy() {
    let host = Host::start();

    host.run(&["compliance", "risks", "accept", "r1"]);
    let second = host.run(&["compliance", "risks", "accept", "r2"]);

    assert_eq!(second.code, 0, "{second:#?}");
    assert_eq!(host.description_fetches(), 1);
}

#[test]
fn a_copy_older_than_an_hour_is_fetched_again() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(61 * 60));

    let run = host.run(&["compliance", "risks", "accept", "r2"]);
    let after = host.run(&["compliance", "risks", "accept", "r3"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(after.code, 0, "{after:#?}");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_stale_copy_is_used_when_the_fetch_fails() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["compliance", "risks", "accept", "r2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/risks/r2/accept");
}

#[test]
fn no_copy_and_no_fetch_is_a_network_error() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["compliance", "risks", "accept", "r1"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
    assert!(host.api_requests().is_empty());
}

#[test]
fn hand_written_commands_never_fetch_the_description() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["compliance", "api", "GET", "/api/v1/risks"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn an_operation_added_after_caching_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);
    host.serve(Some(with_operation(
        "risks.archive",
        "/api/v1/risks/{id}/archive",
    )));

    let run = host.run(&["compliance", "risks", "archive", "r1"]);
    let again = host.run(&["compliance", "risks", "archive", "r2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(again.code, 0, "{again:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/risks/r1/archive");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_new_group_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);
    host.serve(Some(with_operation(
        "suppliers.terminate",
        "/api/v1/suppliers/{id}/terminate",
    )));

    let run = host.run(&["compliance", "suppliers", "terminate", "s1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/suppliers/s1/terminate");
}

#[test]
fn a_command_still_unknown_after_the_refetch_is_a_usage_mistake() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);

    let action = host.run(&["compliance", "risks", "explode", "r1"]);
    let group = host.run(&["compliance", "dragons", "list"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(action.stderr_json()["error"], "usage");
    assert_eq!(group.code, 2, "{group:#?}");
    assert_eq!(host.description_fetches(), 3);
    assert_eq!(host.api_requests().len(), 1);
}

#[test]
fn the_accounts_group_inside_a_product_is_generated_like_any_other() {
    let host = Host::start();

    let list = host.run(&["compliance", "accounts", "list", "--top", "1"]);
    let help = host.run(&["compliance", "accounts", "--help"]);

    assert_eq!(list.code, 0, "{list:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/accounts?$top=1");
    for expected in ["list", "switch", "get", "Get an Account"] {
        assert!(
            help.stdout.contains(expected),
            "{expected}: {}",
            help.stdout
        );
    }
}

#[test]
fn the_hand_written_commands_of_a_group_hide_generated_ones_of_their_name() {
    let host = Host::start();
    let operation = |operation_id: &str| {
        json!({
            "post": {
                "operationId": operation_id,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
            }
        })
    };
    let mut document = fixture();
    document["paths"]["/api/v1/api/{id}"] = operation("api.run");
    document["paths"]["/api/v1/chat/{id}/send"] = operation("chat.send");
    document["paths"]["/api/v1/chat/{id}/history"] = operation("chat.history");
    host.serve(Some(document));

    let raw = host.run(&["compliance", "api", "GET", "/api/v1/risks"]);
    let hidden_group = host.run(&["compliance", "api", "run", "x"]);
    let chat_help = host.run(&["compliance", "chat", "--help"]);
    let generated_action = host.run(&["compliance", "chat", "history", "c1"]);

    assert_eq!(raw.code, 0, "{raw:#?}");
    assert_eq!(hidden_group.code, 2, "{hidden_group:#?}");
    assert_eq!(chat_help.code, 0, "{chat_help:#?}");
    assert!(chat_help.stdout.contains("history"), "{}", chat_help.stdout);
    assert!(
        chat_help.stdout.contains("Send one message"),
        "{}",
        chat_help.stdout
    );
    assert_eq!(generated_action.code, 0, "{generated_action:#?}");
    let urls: Vec<String> = host
        .api_requests()
        .into_iter()
        .map(|request| request.url)
        .collect();
    assert_eq!(urls, ["/api/v1/risks", "/api/v1/chat/c1/history"]);
}

#[test]
fn a_generated_chat_send_is_not_hidden_in_a_product_without_the_chat_operations() {
    let host = Host::start();
    host.serve(Some(without_operations(
        with_operation("chat.send", "/api/v1/chat/{id}/send"),
        &["conversations.create"],
    )));

    let run = host.run(&["compliance", "chat", "send", "c1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/chat/c1/send");
}

#[test]
fn a_download_writes_the_bytes_to_out_and_prints_a_summary() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();

    let run = host.run_in(
        dir.path(),
        &[
            "compliance",
            "evidence",
            "download",
            "e1",
            "--out",
            "scan.pdf",
        ],
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

    let run = host.run(&["compliance", "evidence", "download", "e1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn out_on_a_command_that_answers_json_is_a_usage_mistake() {
    let host = Host::start();

    let run = host.run(&["compliance", "risks", "accept", "r1", "--out", "risk.json"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn an_upload_target_field_says_so_in_help() {
    let host = Host::start();

    let run = host.run(&["compliance", "evidence", "create", "--help"]);

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

    let run = mvdm_in(
        &std::env::temp_dir(),
        &["compliance", "risks", "accept", "r1"],
        &[
            ("MVDM_COMPLIANCE_URL", &server.url()),
            ("MVDM_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(run.stdout_json()["handoffUrl"], "https://example.test/go");
}

#[test]
fn an_integer_enum_is_checked_and_sent_as_a_number() {
    let host = Host::start();

    let wrong = host.run(&["compliance", "risks", "create", "--impact", "4"]);
    let right = host.run(&["compliance", "risks", "create", "--impact", "2"]);

    assert_eq!(wrong.code, 2, "{wrong:#?}");
    assert_eq!(right.code, 0, "{right:#?}");
    assert_eq!(body_json(&host.only_api_request()), json!({ "impact": 2 }));
}

#[test]
fn help_without_the_description_lists_the_hand_written_commands_and_says_what_is_missing() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["compliance", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(run.stdout.contains("api"), "{}", run.stdout);
    // `chat` needs the description to know the Product holds its operations.
    assert!(!run.stdout.contains("chat"), "{}", run.stdout);
    assert!(!run.stdout.contains("risks"), "{}", run.stdout);
    assert!(
        run.stdout.contains("openapi/v1.json are missing"),
        "{}",
        run.stdout
    );
    assert_ends_with_the_docs_address(&host, &run);
}

fn assert_ends_with_the_docs_address(host: &Host, run: &Run) {
    let expected = format!("Docs: {}/agents", host.server.url());
    assert!(run.stdout.trim_end().ends_with(&expected), "{}", run.stdout);
}

#[test]
fn help_ends_with_the_docs_address_on_the_current_host() {
    let host = Host::start();

    let run = host.run(&["compliance", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_ends_with_the_docs_address(&host, &run);
}

#[test]
fn an_unknown_command_with_a_failed_fetch_does_not_fetch_twice() {
    let host = Host::start();
    host.run(&["compliance", "risks", "accept", "r1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["compliance", "risks", "explode", "r1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(host.description_fetches(), 2);
}

/// The E2E suite predicts the CLI's commands with copies of its naming rules (`tests/e2e/operations.rs`). On the
/// fixture, and on a few operations that each test one rule, every command they predict is the one the binary
/// builds: the same names, the same hidden ones, and `--out` on the same downloads.
#[test]
fn the_e2e_naming_rules_name_the_commands_the_cli_builds() {
    let mut document = fixture();
    for (path, operation_id, media_type) in [
        (
            "/api/v1/risk-assessments/by-url",
            "riskAssessments.listByURLPath",
            "application/json",
        ),
        (
            "/api/v1/check_runs/v2",
            "check_runs.getV2Items",
            "application/problem+json",
        ),
        (
            "/api/v1/reports/pdf",
            "reports.exportPDF",
            "application/pdf",
        ),
        // A name an earlier operation holds: the CLI keeps the first, which answers JSON.
        ("/api/v1/risks/export", "risks.list", "text/csv"),
        // Hidden: an action named `help`, a group named `help`, and a hand-written command without actions.
        ("/api/v1/risks/help", "risks.help", "application/json"),
        ("/api/v1/help", "help.list", "application/json"),
        ("/api/v1/api", "api.get", "application/json"),
    ] {
        document["paths"][path] = json!({
            "get": {
                "operationId": operation_id,
                "responses": { "200": { "description": "OK", "content": { media_type: {} } } }
            }
        });
    }

    let mut predicted: Vec<(CommandName, bool)> = Vec::new();
    let mut hidden = Vec::new();
    let live = live_operations(&document);
    let holds_chat = holds_chat(&live);
    for operation in live {
        let Some(command) = operation.command else {
            continue;
        };
        if !command.is_generated(holds_chat) {
            hidden.push(command);
        } else if !predicted.iter().any(|(earlier, _)| *earlier == command) {
            predicted.push((command, operation.download));
        }
    }
    let mut groups: Vec<&str> = Vec::new();
    for (command, _) in &predicted {
        if !groups.contains(&command.group.as_str()) {
            groups.push(&command.group);
        }
    }

    let host = Host::start();
    host.serve(Some(document));
    // Without the description, only the hand-written commands answer.
    let hand_written = Host::start();
    hand_written.serve(None);

    let root = listed_commands(&host.run(&["compliance", "--help"]));
    for group in &groups {
        assert!(
            root.iter().any(|listed| listed == group),
            "{group}: {root:?}"
        );

        let mut expected: Vec<String> =
            listed_commands(&hand_written.run(&["compliance", group, "--help"]));
        for (command, _) in predicted
            .iter()
            .filter(|(command, _)| command.group == *group)
        {
            expected.push(command.action.clone());
        }
        expected.sort();
        expected.dedup();
        let mut listed = listed_commands(&host.run(&["compliance", group, "--help"]));
        listed.sort();
        assert_eq!(listed, expected, "mvdm {group} --help");
    }
    for (command, download) in &predicted {
        let run = host.run(&["compliance", &command.group, &command.action, "--help"]);
        assert_eq!(run.code, 0, "{command} --help: {run:#?}");
        let takes_out = run
            .stdout
            .lines()
            .any(|line| line.trim_start().starts_with("--out "));
        assert_eq!(takes_out, *download, "{command} --help: {}", run.stdout);
    }
    for command in &hidden {
        let listed = listed_commands(&host.run(&["compliance", &command.group, "--help"]));
        assert!(
            !listed.contains(&command.action),
            "{command} is not hidden: {listed:?}"
        );
    }
}

/// The command names a `--help` lists under `Commands:`, or none when it fails or lists none.
fn listed_commands(run: &Run) -> Vec<String> {
    if run.code != 0 {
        return Vec::new();
    }
    run.stdout
        .lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}
