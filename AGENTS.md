# AGENTS.md

Rust crate `compliance-cli`, binary `compliance`: a command-line tool for the Compliance REST API.

- Finish each change green on CI's checks: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test`.

## Reach for

- **Coding standards** — before changing code, tests, docs, or dependencies:
  [`CODING_STANDARDS.md`](CODING_STANDARDS.md).
- **API facts** — any fact about what the API does: read the Compliance and Auth server source in `../mvdmio-suite`
  (`Compliance/`, `Auth/`, `Libraries/`).
- **Layout** — find the module that owns a concern: [`.agents/refs/layout.md`](.agents/refs/layout.md).
- **Test-bed** — run commands against a real Compliance with the Launcher, or work on the E2E suite or its nightly
  run: [`.agents/refs/test-bed.md`](.agents/refs/test-bed.md).
- **Release** — cut one, or change how one is built: [`.agents/refs/release.md`](.agents/refs/release.md).
- **Tracker** — read or write an Issue: [`.agents/refs/tracker.md`](.agents/refs/tracker.md).
