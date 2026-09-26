# 06 — Nightly E2E workflow

Status: pending

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

- [ ] `.github/workflows/e2e.yml` runs on a nightly schedule and on `workflow_dispatch`, on `ubuntu-latest`.
- [ ] It checks out `mvdmio-suite` `master` with `MVDMIO_SUITE_TOKEN` and LFS smudging off, installs .NET 10, and
      sets `MVDMIO_SUITE_DIR`.
- [ ] It runs `cargo test --test e2e -- --ignored` and uploads `target/e2e/` as an artifact, even on failure.
- [ ] The workflow file is valid YAML (and passes `actionlint` if it is installed); `ci.yml` is unchanged.
- [ ] `AGENTS.md` mentions the nightly workflow and its secret.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
