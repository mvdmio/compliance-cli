# compliance-cli

- Finish each change green on the checks in `.github/workflows/ci.yml`, which CI also runs on macOS and Windows.

## Context

- Before writing code, tests, or docs, or changing `Cargo.toml` → [`CODING_STANDARDS.md`](CODING_STANDARDS.md).
- Before relying on a fact about what the API does → the Compliance and Auth server source in `../mvdmio-suite`.
- Before running `compliance` against a real Compliance (a Test-bed), or running the E2E suite → the Launcher's
  header, in `scripts/test-bed.sh`.
- Before reading or writing an Issue → [`.agents/refs/tracker.md`](.agents/refs/tracker.md).
