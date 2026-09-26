# 04 — Help sweep

Status: done

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

- [x] The Help sweep asks every operation in the live description for `--help`, and passes against a fresh
      Test-bed.
- [x] A failing command does not stop the sweep; the failure lists every command that failed.
- [x] Every `--help` run appears in the timing report.
- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the sweep
      listed as ignored.

## Outcome

- `tests/e2e/sweeps.rs` holds the Help sweep (`help_sweep`) and the pieces step 05 reuses: `live_operations`
  (every operation in document order, read the way `operations` in `src/openapi.rs` reads them: the same seven
  methods, `$ref` path items resolved) and `CommandName` (`from_operation_id`, the CLI's rule: split on the first
  `.`, kebab-case each part; `is_generated`, which follows `tree` in `src/generated.rs`). `LiveOperation` holds only
  the command for now; step 05 adds the method and path it needs, which would be dead code here.
- `TestBed::description` in `tests/e2e/test_bed.rs` fetches `<url>/openapi/v1.json` without a credential, through
  `ureq`, which the crate already depends on.
- The crate has no library target, so `kebab_case`, `resolve`, the method list, and the list of hand-written
  commands without actions (`login`, `logout`, `status`, `api`, `skill`) are copies of the CLI's. The module doc
  names where each comes from.
- The sweep asks every generated command once: two operations with one name are one command, as the CLI keeps the
  first. `accounts list` and `accounts switch` are asked like the rest and reach the hand-written commands. An
  operation with no `<group>.<action>` `operationId`, or one that `tree` hides (a `help` name, a group behind a
  hand-written command without actions), yields no generated command and is not asked; the Read sweep in step 05
  is where a dropped operation fails.
- A command passes when it exits 0 and prints `Usage:` on stdout. The sweep collects every failure and fails once,
  listing them all with the count of commands asked.
- Drift from the Footprint: `tests/e2e/test_bed.rs` gained `TestBed::description`.
- A run against a Test-bed started by the Launcher passed all nine Scenarios. The live description had 403
  operations, all generated commands; the timing report held 403 `--help` runs, all exit 0, in about 65 seconds.
