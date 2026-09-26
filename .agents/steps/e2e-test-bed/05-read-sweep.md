# 05 — Read sweep

Status: pending

## What to build

The Read sweep: a Scenario that runs every GET operation in the live `/openapi/v1.json` through its generated
command against the Test-bed.

- **Naming**: the command name comes from the step 04 rule. When `compliance <group> --help` does not list the
  action, that read fails: the CLI dropped an operation. `accounts.list` runs as the hand-written `accounts list`.
- **Ids**: a read with path parameters first runs the list whose path the read extends (for
  `/api/v1/risks/{id}/comments`, the list at `/api/v1/risks`), then takes the first item's `id`. A small table
  covers the cases the paths cannot explain:
  - `check-runs/{id}` takes `checkRunId` from `check-results`,
  - a requirement `{code}` takes `code` from the framework's requirements list,
  - conversation messages take the id from the conversations list.

  Ids come only from the Test-bed's own data, never hard-coded, so the sweep works on any data.
- **Skipped**: when the list a read depends on is empty, as `import-sessions` always is after the seed, every read
  under it is reported as skipped, naming the empty list. It does not fail.
- **Pass rule**: a read passes when it exits 0 and prints valid JSON. The two downloads (`evidence download`,
  `attachments download`) pass when they write a non-empty file through `--out` into a temp folder. No answer is
  checked against a response schema.
- **Report**: the sweep runs every read before it fails. It prints the passed, skipped, and failed reads with
  reasons, then fails once, listing every failure. Every command, the lists that find ids included, appears in the
  timing report.

If a read fails on the seeded data for a reason that is not the CLI's, do not weaken the pass rule. Record it in
`## Outcome` as a Compliance-side finding.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `tests/e2e/sweeps.rs` — the Read sweep, the id table, skip reporting
- `tests/e2e/main.rs` — register the Scenario
- `src/openapi.rs` — read-only: `is_download`, path placeholders
- `src/dispatch.rs`, `src/generated.rs` (`OUT`, `path_id`, `query_id`) — read-only: how path parameters and `--out` are passed

## Acceptance criteria

- [ ] Every GET operation in the live description is run, or reported as skipped because the list it depends on is
      empty.
- [ ] An operation with no matching command in `--help` fails the sweep.
- [ ] Ids are found from the Test-bed's lists and the three-entry table, with nothing hard-coded.
- [ ] Both downloads write a non-empty file through `--out`.
- [ ] The sweep passes against a fresh Test-bed, or every remaining failure is recorded in `## Outcome` as a
      Compliance-side finding.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the sweep
      listed as ignored.
