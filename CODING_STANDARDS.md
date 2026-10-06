# Coding Standards

## Output

- Every output is JSON, except `--help`, `--version`, and `compliance skill` (Markdown).
- Exit codes: 0 success, 1 error, 2 usage, 3 Browser handoff.

## Network

- TLS is rustls only: `cargo tree -i openssl-sys` must find nothing.
- No telemetry.

## Tests

- Tests drive the binary as an Agent does: arguments and environment in; stdout, stderr, the exit code, and the
  requests that reached the Fake hosts out. They never check internal types.
- The tests never open a real browser: they run without a display, and the browser test on Linux puts a fake
  `xdg-open` first on `PATH`.
