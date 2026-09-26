//! The smoke Scenarios: one per CLI surface. Each checks what the CLI promises (the exit code, valid JSON, and the
//! fields the command names), never Compliance's business rules.

use serde_json::{Value, json};

use crate::test_bed::Scenario;

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn status() {
    let scenario = Scenario::start("status");

    let run = scenario.run(&["status"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let status = run.stdout_json();
    assert_eq!(status["host"], json!(scenario.test_bed().url), "{run:#?}");
    assert_eq!(status["signedIn"], json!(true), "{run:#?}");
    assert_eq!(
        status["credential"],
        json!({ "kind": "personal-token", "source": "COMPLIANCE_TOKEN" }),
        "{run:#?}"
    );
    // A Personal token carries no `id_token`, so `status` names no User for it.
    assert_eq!(status["user"], json!(null), "{run:#?}");
    assert_eq!(status["account"]["id"], json!(1), "{run:#?}");
    assert_eq!(status["account"]["current"], json!(true), "{run:#?}");
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn accounts() {
    let scenario = Scenario::start("accounts");

    let list = scenario.run(&["accounts", "list"]);
    assert_eq!(list.code, 0, "{list:#?}");
    let listed = list.stdout_json();
    assert_eq!(current_account(&listed), Some(1), "{list:#?}");
    assert!(
        lists_account(&listed, 1) && lists_account(&listed, 2),
        "{list:#?}"
    );

    // Account 2 has no Compliance License, so nothing but the switch back runs while the token is there.
    let back = BackToAccountOne(&scenario);
    let switch = scenario.run(&["accounts", "switch", "2"]);
    assert_eq!(switch.code, 0, "{switch:#?}");
    assert_eq!(
        current_account(&switch.stdout_json()),
        Some(2),
        "{switch:#?}"
    );
    drop(back);
}

/// Switches the shared Personal token back to Account 1 when dropped, so a failed assertion on Account 2 never
/// leaves the token there for the next Scenario.
struct BackToAccountOne<'a>(&'a Scenario);

impl Drop for BackToAccountOne<'_> {
    fn drop(&mut self) {
        let run = self.0.run(&["accounts", "switch", "1"]);
        // A second panic while unwinding aborts the whole run, so a failing Scenario only says so on stderr.
        if std::thread::panicking() {
            if run.code != 0 {
                eprintln!(
                    "the switch back to Account 1 failed; the token may be on Account 2: {run:#?}"
                );
            }
            return;
        }
        assert_eq!(run.code, 0, "{run:#?}");
        assert_eq!(current_account(&run.stdout_json()), Some(1), "{run:#?}");
    }
}

/// The array `value[field]`, or an empty one when it is missing or not an array.
fn array_field<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value[field].as_array().map_or(&[], Vec::as_slice)
}

/// Whether `value[field]` is a non-empty string, as an id is.
fn has_text(value: &Value, field: &str) -> bool {
    value[field].as_str().is_some_and(|text| !text.is_empty())
}

/// The id of the Account an Account list marks `current`.
fn current_account(list: &Value) -> Option<i64> {
    array_field(list, "items")
        .iter()
        .find(|item| item["current"] == true)?["id"]
        .as_i64()
}

fn lists_account(list: &Value, id: i64) -> bool {
    array_field(list, "items")
        .iter()
        .any(|item| item["id"] == id)
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn api() {
    let scenario = Scenario::start("api");

    let list = scenario.run(&["api", "GET", "/api/v1/risks"]);
    assert_eq!(list.code, 0, "{list:#?}");
    assert!(list.stdout_json()["items"].is_array(), "{list:#?}");

    let missing = scenario.run(&[
        "api",
        "GET",
        "/api/v1/risks/00000000-0000-0000-0000-000000000000",
    ]);
    assert_eq!(missing.code, 1, "{missing:#?}");
    assert_eq!(missing.stdout, "", "{missing:#?}");
    let problem = missing.stderr_json();
    assert_eq!(problem["status"], json!(404), "{missing:#?}");
    assert!(problem["type"].is_string(), "{missing:#?}");
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn generated_write_with_body() {
    let scenario = Scenario::start("generated write with --body");

    // Only the fields `risks.create` requires. The Risk stays on the Test-bed.
    let run = scenario.run(&[
        "risks",
        "create",
        "--body",
        r#"{"title":"E2E smoke Risk","inherentLikelihood":2,"inherentImpact":3}"#,
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let risk = run.stdout_json();
    assert!(has_text(&risk, "id"), "{run:#?}");
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn bad_token() {
    let scenario = Scenario::start("bad token");

    // Well formed (`mvdm_pat_` and 32 bytes in base64url), but no row on the Test-bed holds its hash.
    let token = format!("mvdm_pat_{}", "A".repeat(43));
    let run = scenario.run_with_token(&["status"], &token);

    assert_eq!(run.code, 1, "{run:#?}");
    assert!(run.stderr_json().is_object(), "{run:#?}");
}

/// One upload part as Compliance issues it: 25 MiB, `ImportUploadStagingService.PartSizeBytes` in `mvdmio-suite`.
/// Should that grow, the part-sized upload goes in one request, and this must follow it.
const PART_BYTES: usize = 25 * 1024 * 1024;

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn file_to_evidence_in_one_request() {
    let scenario = Scenario::start("--file to Evidence, one request");
    upload_evidence(&scenario, "small.txt", 4 * 1024);
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn file_to_evidence_in_parts() {
    let scenario = Scenario::start("--file to Evidence, in parts");
    upload_evidence(&scenario, "large.txt", PART_BYTES + 1024);
}

/// Writes a file of `length` bytes and runs `evidence create --file` on it. Evidence accepts 50 MiB per file, so it
/// can take a file larger than one part; a Conversation upload (25 MiB) cannot. The Evidence stays on the Test-bed.
fn upload_evidence(scenario: &Scenario, name: &str, length: usize) {
    let folder = tempfile::tempdir().expect("a temporary folder for the file");
    let path = folder.path().join(name);
    let bytes: Vec<u8> = (b'a'..=b'z').cycle().take(length).collect();
    std::fs::write(&path, bytes).expect("write the file to upload");
    let path = path.to_str().expect("a UTF-8 temporary path");

    // `title` is the one field `evidence.create` requires beside the upload.
    let run = scenario.run(&[
        "evidence",
        "create",
        "--file",
        path,
        "--title",
        "E2E smoke Evidence",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let evidence = run.stdout_json();
    assert!(has_text(&evidence, "id"), "{run:#?}");
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn chat_send() {
    let scenario = Scenario::start("chat send");

    let run = scenario.run(&["chat", "send", "Hello from the E2E suite."]);

    // Scripted Turns still debit Account 1's Assistant allowance, which the Launcher funds.
    let out_of_funds = run
        .stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|problem| problem["status"] == 402);
    assert!(
        !out_of_funds,
        "chat send answered 402: the Test-bed's Assistant allowance for Account 1 ran out; this is not a CLI \
         failure. {run:#?}"
    );
    assert_eq!(run.code, 0, "{run:#?}");
    // `chat send` prints only once the turn is idle, so an answer at all says the conversation is idle.
    let chat = run.stdout_json();
    assert!(has_text(&chat, "conversationId"), "{run:#?}");
    assert!(
        array_field(&chat, "messages")
            .iter()
            .any(|message| message["role"] == "assistant"),
        "{run:#?}"
    );
}
