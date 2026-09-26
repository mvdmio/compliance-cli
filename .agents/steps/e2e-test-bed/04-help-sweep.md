# 04 — Help sweep

Status: pending

## What to build

The Help sweep: a Scenario that fetches the live `/openapi/v1.json` from the Test-bed and runs
`compliance <group> <action> --help` for every operation in it. Group and action come from the CLI's rule: the
`operationId` is split on its first `.`, and each part is kebab-cased. Operations that hand-written commands replace
(`accounts list`, `accounts switch`) are asked for help the same way.

Each command passes when it exits 0 and prints help text. The sweep runs every command before it fails, then fails
once, listing every command that did not pass. The pieces that fetch the description and derive command names live
where the Read sweep in step 05 can reuse them.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `tests/e2e/sweeps.rs` — new: the Help sweep, fetching the live description, deriving command names
- `tests/e2e/main.rs` — register the Scenario
- `src/openapi.rs` — read-only: `kebab_case`, how an operation is named
- `src/generated.rs` — read-only: `tree`, which operations become commands

## Acceptance criteria

- [ ] The Help sweep asks every operation in the live description for `--help`, and passes against a fresh
      Test-bed.
- [ ] A failing command does not stop the sweep; the failure lists every command that failed.
- [ ] Every `--help` run appears in the timing report.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the sweep
      listed as ignored.
