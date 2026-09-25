# AGENTS.md

Rust crate `compliance-cli`, binary `compliance`: a command-line tool for the Compliance REST API.

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt` (CI runs `cargo fmt --check`)

CI runs format, lint, and test on Linux, macOS, and Windows for every push and pull request.

## Layout

- `src/main.rs`: argument parsing, the choice between hand-written and generated commands, and the exit-code
  mapping.
- `src/cli.rs`: the command line.
- `src/config.rs`: the host.
- `src/credential.rs`: the credential in use (`COMPLIANCE_TOKEN`, else the stored sign-in) and its refresh.
- `src/store.rs`: the stored sign-in file (`credentials.json`, one entry per host, mode 0600 on Unix).
- `src/discovery.rs`: finds Auth through the Protected Resource Metadata.
- `src/oauth.rs`: token, device, and revocation requests, and PKCE.
- `src/http.rs`: the HTTP client (User-Agent, bearer, 429 retry, refresh before expiry and on a 401).
- `src/response.rs`: turns an API answer into output.
- `src/description.rs`: the API's OpenAPI description, cached per host for an hour and fetched again for an
  unknown command.
- `src/openapi.rs`: reads operations, parameters, and body fields out of the description.
- `src/generated.rs`: builds the generated commands beside the hand-written ones. `src/dispatch.rs`: turns their
  arguments into the request.
- `src/upload.rs`: `--file`, which sends a file through an Upload link, in parts when it is large, and resumes.
- `src/failure.rs`, `src/output.rs`: errors, exit codes, and JSON printing.
- `src/login.rs`, `src/logout.rs`, `src/status.rs`, `src/accounts.rs`, `src/chat.rs`, `src/api.rs`,
  `src/skill.rs`: the commands.
- `SKILL.md`: the Agent skill file. `compliance skill` prints it, built into the binary, so a change to it ships
  with the next release.
- `tests/fixtures/openapi.json`: the API description the generated-command tests serve.
- `tests/`: tests that run the built binary against in-process fake Compliance and Auth hosts (`tests/support`).
  The tests never open a real browser: they run without a display, and the browser test on Linux puts a fake
  `xdg-open` first on `PATH`.

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

## Rules

- Every output is JSON, except `--help`, `--version`, and `compliance skill` (Markdown). Exit codes: 0 success, 1 error, 2 usage, 3 Browser
  handoff.
- Tests drive the binary as an Agent does: arguments and environment in; stdout, stderr, the exit code, and the
  requests that reached the fake server out. They never check internal types.
- TLS is rustls only: `cargo tree -i openssl-sys` must find nothing.
- No telemetry.
