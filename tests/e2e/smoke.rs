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

/// The `items` of an Account list.
fn accounts_in(list: &Value) -> &[Value] {
    list["items"].as_array().map_or(&[], Vec::as_slice)
}

/// The id of the Account an Account list marks `current`.
fn current_account(list: &Value) -> Option<i64> {
    accounts_in(list)
        .iter()
        .find(|item| item["current"] == true)?["id"]
        .as_i64()
}

fn lists_account(list: &Value, id: i64) -> bool {
    accounts_in(list).iter().any(|item| item["id"] == id)
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
    assert!(
        risk["id"].as_str().is_some_and(|id| !id.is_empty()),
        "{run:#?}"
    );
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
