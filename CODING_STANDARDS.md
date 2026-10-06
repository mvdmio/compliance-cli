# Coding Standards

## Platforms

- Write code and tests that pass on Linux, macOS, and Windows: CI checks every push and pull request on all three.

## Output

- Every output is JSON, except `--help`, `--version`, and `compliance skill` (Markdown).
- Exit codes: 0 success, 1 error, 2 usage, 3 Browser handoff.

## Network

- TLS is rustls only: `cargo tree -i openssl-sys` must find nothing.
- Every request serves the command the user ran. That keeps the README's promise: no telemetry, no crash reports, no
  update checks.

## Tests

- Tests drive the built binary as an Agent does, against in-process Fake hosts for Compliance and Auth
  (`tests/support`): arguments and environment go in, and assertions read what comes out — stdout, stderr, the exit
  code, and the requests that reached the Fake hosts.
- Every browser a test opens is a fake. Tests run without a display (`tests/support` removes `DISPLAY` and
  `WAYLAND_DISPLAY`), so `login` takes the device code. macOS and Windows always have a browser to open, so a test
  that runs `login` without `--device` carries a `#[cfg]` that leaves them out; the browser sign-in tests run on
  Linux only, with a fake `xdg-open` first on `PATH`.
- Change a rule that `tests/e2e/operations.rs` copies from `src/` (its `//!` doc lists them) together with its copy:
  `tests/generated.rs` checks the copies against the binary on every push.
- Mark each E2E Scenario `#[ignore]` with the reason the others carry, so plain `cargo test` lists it without running
  it and needs no .NET, Docker, or Postgres.

## Docs

- `SKILL.md`, the Agent skill file, ships inside the binary: `compliance skill` prints it, so an edit to it reaches
  users with the next release.

## Releases

- Change the release build in `dist-workspace.toml`, then run `dist generate`, which writes
  `.github/workflows/release.yml` from it.
- Keep the installer names, `compliance-cli-installer.sh` and `compliance-cli-installer.ps1` (dist names them after
  the package): the install lines always fetch them from the latest release.
