# 02 — Account, api, write, and bad-token Scenarios

Status: pending

## What to build

Four more smoke Scenarios in the E2E suite. Each runs the real binary against the Test-bed through the harness from
step 01 (its lock, its per-Scenario folder, its timing entries). Each checks only what the CLI promises: the exit
code, valid JSON, and the fields the command names. None checks Compliance's business rules.

- **Accounts**: `accounts list` exits 0 and lists Account 1 (current) and seed Account 2. `accounts switch 2`
  exits 0 and answers with Account 2; `accounts switch 1` then exits 0 and answers with Account 1. The switch moves
  the shared Personal token, and Account 2 has no Compliance License, so nothing else runs while on Account 2. The
  switch back to Account 1 happens even when an assertion in between fails (a guard that switches back on drop).
- **api**: `api GET /api/v1/<a list path>` exits 0 and prints JSON. `api GET` on an id that does not exist exits 1
  and reports the problem+json answer (status 404) in the CLI's error JSON on stderr.
- **Generated write with `--body`**: one generated create command run with inline `--body` JSON: `risks create`,
  unless the live description makes another create simpler. It exits 0 and prints the created record's JSON with
  an `id`. Fill only the fields the live description marks required. The record stays on the Test-bed.
- **Bad token**: `status` with `COMPLIANCE_TOKEN` set to a well-formed but unknown `mvdm_pat_…` secret exits 1
  with JSON on stderr.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `tests/e2e/main.rs` — register the new Scenarios
- `tests/e2e/smoke.rs` — new: the smoke Scenarios (move `status` here if it reads better)
- `tests/e2e/test_bed.rs` — run helper: running with a different token for the bad-token Scenario
- `src/accounts.rs`, `src/api.rs`, `src/failure.rs`, `src/response.rs` — read-only: the answer and error shapes
- `/data/projects/mvdmio/mvdmio-suite/Compliance/src/mvdmio.Compliance.Web/Api/` — read-only: `accounts.switch`, `risks.create`

## Acceptance criteria

- [ ] The accounts Scenario lists both Accounts, switches to 2 and back to 1, and the token is on Account 1 after
      the Scenario ends, pass or fail.
- [ ] The api Scenario passes a GET with exit 0, and a 404 with exit 1 and problem+json details on stderr.
- [ ] The write Scenario creates a record through a generated command with `--body` and gets its `id` back.
- [ ] The bad-token Scenario gets exit 1 and JSON on stderr.
- [ ] `cargo test --test e2e -- --ignored` passes every Scenario so far against a fresh Test-bed, and each command
      appears in the timing report.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the new
      Scenarios listed as ignored.
