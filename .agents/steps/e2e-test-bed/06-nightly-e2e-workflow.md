# 06 — Nightly E2E workflow

Status: done

## What to build

A scheduled GitHub Actions workflow, `.github/workflows/e2e.yml`, that runs the E2E suite every night against the
newest Compliance. It also has `workflow_dispatch`, so a person can start its first run by hand to prove the secret.

On `ubuntu-latest` it:

1. checks out this repo at HEAD,
2. checks out `michielvandermeer/mvdmio-suite` `master` with `GIT_LFS_SKIP_SMUDGE=1` (and no LFS fetch), using the
   read-only token secret `MVDMIO_SUITE_TOKEN`, into a folder it then names in `MVDMIO_SUITE_DIR`,
3. installs .NET 10 (`mvdmio-suite` pins no SDK) and the stable Rust toolchain,
4. builds the CLI, then runs `cargo test --test e2e -- --ignored`, which starts the Test-bed through the Launcher
   (Docker is already on the runner),
5. uploads `target/e2e/` as a workflow artifact, also when the run failed.

A failure notifies through GitHub's default notification for scheduled runs; nothing else is added for that. The
existing `ci.yml` is unchanged. `AGENTS.md` says in one line that the E2E suite runs nightly in this workflow and
needs the `MVDMIO_SUITE_TOKEN` secret, which a person sets up once.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `.github/workflows/e2e.yml` — new: the nightly workflow
- `.github/workflows/ci.yml` — read-only: the toolchain steps to match
- `AGENTS.md` — the nightly line near the CI line
- `scripts/test-bed.sh` — read-only: what the runner must provide

## Acceptance criteria

- [x] `.github/workflows/e2e.yml` runs on a nightly schedule and on `workflow_dispatch`, on `ubuntu-latest`.
- [x] It checks out `mvdmio-suite` `master` with `MVDMIO_SUITE_TOKEN` and LFS smudging off, installs .NET 10, and
      sets `MVDMIO_SUITE_DIR`.
- [x] It runs `cargo test --test e2e -- --ignored` and uploads `target/e2e/` as an artifact, even on failure.
- [x] The workflow file is valid YAML (and passes `actionlint` if it is installed); `ci.yml` is unchanged.
- [x] `AGENTS.md` mentions the nightly workflow and its secret.
- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.

## Outcome

- `.github/workflows/e2e.yml` runs at 03:00 UTC every night and on `workflow_dispatch`, on `ubuntu-latest`, with a
  120-minute limit and `contents: read`. It checks out this repo, then `michielvandermeer/mvdmio-suite` `master`
  into `mvdmio-suite/` with `MVDMIO_SUITE_TOKEN`, `lfs: false`, and `GIT_LFS_SKIP_SMUDGE=1`, and sets
  `MVDMIO_SUITE_DIR` to that folder. It installs .NET `10.0.x` (`actions/setup-dotnet@v5`, as `mvdmio-suite`'s own
  `app.yml` does), Bun (`oven-sh/setup-bun@v2`, which the Compliance build needs, per step 01), and stable Rust,
  runs `cargo build`, then `cargo test --test e2e -- --ignored`, and uploads `target/e2e/` as the `e2e-timing`
  artifact with `if: always()`.
- Drift from the Footprint: the workflow also installs Bun, which the Step's list leaves out.
- `AGENTS.md` says under the CI line that the E2E suite runs nightly in `e2e.yml` and needs the
  `MVDMIO_SUITE_TOKEN` secret, set up once by a person. Its Test-bed section now names the three rows the Launcher
  writes (Legal Acceptance, Personal token, and the Assistant allowance with 100 USD of extra funds from step 03).
- `ci.yml` is unchanged. The workflow parses as YAML; `actionlint` is not installed here, so it was not run. The
  workflow has not run on GitHub: its first manual or scheduled run proves the secret.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo test --test e2e --
  --ignored` (a fresh Test-bed, all ten Scenarios, 132 seconds) pass.
- After review: a first step fails with a clear error when `MVDMIO_SUITE_TOKEN` is not set; the header comment says
  GitHub's scheduled-run notice goes to the user who last changed the cron line; `AGENTS.md` notes that GitHub
  turns a schedule off after 60 days without repository activity. Kept against the review, because the Spec asks
  for them: both `GIT_LFS_SKIP_SMUDGE=1` and `lfs: false`, and a `cargo build` step before the suite. No Rust or
  NuGet cache: a cached `target/` could bring back an old `target/e2e/` report into the artifact.
