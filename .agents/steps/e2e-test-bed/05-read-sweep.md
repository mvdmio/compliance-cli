# 05 — Read sweep

Status: done

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

- [x] Every GET operation in the live description is run, or reported as skipped because the list it depends on is
      empty.
- [x] An operation with no matching command in `--help` fails the sweep.
- [x] Ids are found from the Test-bed's lists and the three-entry table, with nothing hard-coded.
- [x] Both downloads write a non-empty file through `--out`.
- [x] The sweep passes against a fresh Test-bed, or every remaining failure is recorded in `## Outcome` as a
      Compliance-side finding.
- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass, with the sweep
      listed as ignored.

## Outcome

- `tests/e2e/sweeps.rs` gains `read_sweep`. `LiveOperation` now also holds `method`, `path`, and `download` (a copy
  of `is_download` in `src/openapi.rs` and `is_json` in `src/response.rs`).
- Each GET operation, in document order, fails when it has no `<group>.<action>` `operationId`, when an earlier
  operation already holds its command name (the CLI would run that one), or when `compliance <group> --help` does
  not list its action under `Commands:`. Checked by hand: hiding the `risks` actions failed all four risk reads.
- Path values: each placeholder takes the `id` of the first item in the list at the path before it, with its
  earlier placeholders filled. `PATH_SOURCES` holds the Spec's three cases (`check-runs/{id}` from `check-results`
  `checkRunId`, a requirement `{code}` from the framework's requirements list, conversation messages from the
  conversations list; the last is what the path rule gives too). A list is a page's `items` or a bare array. A list
  that exits non-zero, prints neither, or whose first item lacks the field fails the read; an empty list skips it,
  naming the list.
- The failure for a name an earlier operation already holds goes past the Spec's "when `--help` does not list that
  command"; it is kept because such an operation is one the CLI dropped (User story 25).
- Drift from the Spec: `invitations.preview` answers 422 without `personIds`, which the description does not mark
  required. A second table, `QUERY_SOURCES`, gives it `--person-ids` from the first Person in `people list`, so the
  sweep's ids stay the Test-bed's own. Compliance-side finding: the description should mark `personIds` required.
- Every command runs once: the lists that find ids are the same runs as those lists' own reads, and each
  `<group> --help` runs once. All of them appear in the timing report.
- Downloads get `--out <temp folder>/<group>-<action>` and pass when the file is non-empty; other reads pass on
  exit 0 and JSON on stdout. The sweep prints the passed, skipped, and failed reads, then fails once, listing every
  failure.
- Drift from the Footprint: none; `tests/e2e/main.rs` needed no change.
- A fresh Test-bed (`cargo test --test e2e -- --ignored`, no `COMPLIANCE_E2E_*`) passed all ten Scenarios. The Read
  sweep ran 104 reads: 102 passed, 2 skipped (`import-sessions get` and `tests get`, whose lists are empty after the
  seed), 0 failed, in about 25 seconds.
