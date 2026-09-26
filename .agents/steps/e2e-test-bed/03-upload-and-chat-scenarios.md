# 03 — Upload and chat Scenarios

Status: pending

## What to build

The two smoke Scenarios that run the CLI's longer flows end to end against the Test-bed.

- **`--file` to Evidence**: the generated Evidence create command (`evidence create --file …`, or whatever the live
  description names the upload slot) runs twice, each time with a file written into a temp folder:
  - a small file (a few KiB), which the CLI sends in one request,
  - a file just over one 25 MiB part (for example 25 MiB + 1 KiB), which the CLI sends in parts.

  Each exits 0 and prints the created Evidence as JSON with an `id`. Use Evidence, not a Conversation upload:
  Evidence takes 50 MiB per file, a Conversation upload only 25 MiB.
- **`chat send`**: `chat send "<text>"` exits 0 once the conversation is idle, and prints JSON with
  `conversationId`, `turnState`, and at least one Assistant message. The Test-bed's Assistant is the scripted model,
  so no live language model is called. A 402 answer means the seeded Assistant allowance ran out, not a CLI
  failure; the Scenario's failure message says so when it sees one.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `tests/e2e/smoke.rs` — the upload and chat Scenarios
- `tests/e2e/main.rs` — register them if needed
- `src/upload.rs`, `src/generated.rs` (`upload_slot`, `file_option`) — read-only: the `--file` option and part size
- `src/chat.rs` — read-only: what idle means and the printed shape
- `/data/projects/mvdmio/mvdmio-suite/Compliance/src/mvdmio.Compliance.Web/Api/UploadsController.cs` — read-only: limits and part size
- `/data/projects/mvdmio/mvdmio-suite/Compliance/src/mvdmio.Compliance.Web/Services/Assistant/DevelopmentScriptedModel.cs` — read-only: what the scripted model answers

## Acceptance criteria

- [ ] The small-file Evidence upload exits 0 with the created Evidence's `id`.
- [ ] The part-sized Evidence upload (over 25 MiB) exits 0 with the created Evidence's `id`.
- [ ] `chat send` exits 0 with an idle conversation and an Assistant message in the JSON.
- [ ] `cargo test --test e2e -- --ignored` passes every Scenario so far against a fresh Test-bed.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the new
      Scenarios listed as ignored.
