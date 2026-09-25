# AGENTS.md

Rust crate `compliance-cli`, binary `compliance`: a command-line tool for the Compliance REST API.

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt` (CI runs `cargo fmt --check`)

CI runs format, lint, and test on Linux, macOS, and Windows for every push and pull request.

## Layout

- `src/main.rs`: argument parsing and the exit-code mapping.
- `src/cli.rs`: the command line.
- `src/config.rs`: the host and the credential.
- `src/http.rs`: the HTTP client (User-Agent, bearer, 429 retry).
- `src/response.rs`: turns an API answer into output.
- `src/failure.rs`, `src/output.rs`: errors, exit codes, and JSON printing.
- `src/api.rs`: `compliance api`.
- `tests/`: tests that run the built binary against an in-process fake server (`tests/support`).

## Rules

- Every output is JSON, except `--help` and `--version`. Exit codes: 0 success, 1 error, 2 usage, 3 Browser
  handoff.
- Tests drive the binary as an Agent does: arguments and environment in; stdout, stderr, the exit code, and the
  requests that reached the fake server out. They never check internal types.
- TLS is rustls only: `cargo tree -i openssl-sys` must find nothing.
- No telemetry.
