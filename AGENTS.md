# AGENTS.md

Rust crate `compliance-cli`, binary `compliance`: a command-line tool for the Compliance REST API.

Before writing code or tests, or adding a dependency → `CODING_STANDARDS.md`.

## Commands

- Lint: `cargo clippy --all-targets -- -D warnings`
- E2E suite: `cargo test --test e2e -- --ignored` (see Test-bed below)
- Launcher: `scripts/test-bed.sh` (see Test-bed below)

CI runs format, lint, and test on Linux, macOS, and Windows for every push and pull request. The E2E suite runs
nightly (and on manual dispatch) in `.github/workflows/e2e.yml`, which needs the `MVDMIO_SUITE_DEPLOY_KEY` secret:
the private half of the read-only deploy key "compliance-cli E2E (read-only)" on `mvdmio-suite`, set up once by a
person. GitHub turns the schedule off after 60 days without repository activity; re-enable it in the Actions tab.

## Layout

- `src/main.rs`: argument parsing (a parser error is a usage mistake) and the choice between hand-written and
  generated commands.
- `src/cli.rs`: the command line.
- `src/config.rs`: the host.
- `src/credential.rs`: the credential in use (`COMPLIANCE_TOKEN`, else the stored sign-in) and its refresh.
- `src/store.rs`: the stored sign-in file (`credentials.json`, one entry per host, mode 0600 on Unix).
- `src/discovery.rs`: finds Auth through the Protected Resource Metadata.
- `src/oauth.rs`: token, device, and revocation requests, and PKCE.
- `src/http.rs`: the HTTP client (User-Agent, bearer, 429 retry, refresh before expiry and on a 401).
- `src/response.rs`: turns an API answer into output.
- `src/request.rs`: URL encoding and the `--body` option, shared by the commands that build a request.
- `src/description.rs`: the API's OpenAPI description, cached per host for an hour and fetched again for an
  unknown command.
- `src/openapi.rs`: reads operations, parameters, and body fields out of the description.
- `src/generated.rs`: builds the generated commands beside the hand-written ones.
- `src/dispatch.rs`: turns a generated command's arguments into the request.
- `src/upload.rs`: `--file`, which sends a file through an Upload link, in parts when it is large, and resumes.
- `src/failure.rs`, `src/output.rs`: errors, exit codes, and JSON printing.
- `src/login.rs`, `src/logout.rs`, `src/status.rs`, `src/accounts.rs`, `src/chat.rs`, `src/api.rs`,
  `src/skill.rs`: the commands.
- `SKILL.md`: the Agent skill file. `compliance skill` prints it, built into the binary, so a change to it ships
  with the next release.
- `tests/fixtures/openapi.json`: the API description the generated-command tests serve.
- `CONTEXT.md`: the glossary.
- Compliance and Auth server source: the `mvdmio-suite` monorepo, checked out beside this repo at `../mvdmio-suite`
  (`Compliance/`, `Auth/`, `Libraries/`). Read it for any fact about what the API does.
- `tests/`: tests that run the built binary against in-process fake Compliance and Auth hosts (`tests/support`).
- `scripts/test-bed.sh`: the Launcher, which starts a Test-bed.
- `tests/e2e/`: the E2E suite (`main.rs`: the `e2e` test target, which shares `tests/support`; `smoke.rs`: the
  smoke Scenarios; `sweeps.rs`: the Help sweep and the Read sweep; `operations.rs`: the operations in the live
  description and the commands the CLI names them by, copied from `src/` and checked against the binary in
  `tests/generated.rs`; `test_bed.rs`: finding or starting the Test-bed and running the binary on it; `timing.rs`:
  the timing report under `target/e2e/`).

## Test-bed

A Test-bed is a real Compliance built from `mvdmio-suite`, with its own throwaway Postgres container and data folder.
Every boot reseeds the database, so the Launcher then writes three rows for seed user 1 on Account 1: a Legal
Acceptance, a Personal token, and an Assistant allowance with 100 USD of extra funds for `chat send`. It needs
Docker, the .NET SDK, `bun`, and `curl`, on Linux or macOS.

- `scripts/test-bed.sh` builds and starts one, prints one JSON line `{"url": "...", "token": "..."}` on stdout, and
  runs in the foreground. Ctrl-C or SIGTERM stops Compliance, removes the container, and deletes the folder. Put the
  two values in `COMPLIANCE_URL` and `COMPLIANCE_TOKEN` to run commands against it. Several can run side by side.
- `MVDMIO_SUITE_DIR`: the `mvdmio-suite` checkout the Launcher builds, by default `../mvdmio-suite` next to this repo.
  Set it in a worktree, where that default path does not exist.
- `TEST_BED_BOOT_TIMEOUT`: the seconds the Launcher allows for the build lock, the build, and the boot together
  (default 1800).
- `cargo test --test e2e -- --ignored` runs the E2E suite. With `COMPLIANCE_E2E_URL` and `COMPLIANCE_E2E_TOKEN` set
  (both or neither) it uses that Test-bed; with neither, it runs the Launcher itself and stops it when it ends.
- Plain `cargo test` never runs the E2E suite: it lists its Scenarios as ignored and needs no .NET, Docker, or
  Postgres.

## Releases

`dist` (cargo-dist) builds releases. Its config is `dist-workspace.toml`, and `.github/workflows/release.yml` is
generated from it: change the config, then run `dist generate`, never edit the workflow by hand. `dist plan`
shows what a release builds. The CLI has no self-update: users upgrade by running the install line again.

To cut a release:

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and commit.
2. Tag the commit `v<version>`, such as `v0.2.0`.
3. Push the commit, then the tag: `git push origin v<version>`. The tag push starts the release workflow, which
   builds every target and publishes a GitHub Release with `compliance-cli-installer.sh` and
   `compliance-cli-installer.ps1`. The install lines always fetch the installers of the latest release, so keep
   those names.
