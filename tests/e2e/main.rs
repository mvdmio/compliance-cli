//! The E2E suite: Scenarios that run the real `compliance` binary against a Test-bed. Every Scenario is ignored,
//! so `cargo test` lists them and runs none; `cargo test --test e2e -- --ignored` runs them.

#[path = "../support/mod.rs"]
mod support;
mod test_bed;
mod timing;

use serde_json::json;
use test_bed::Scenario;

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
