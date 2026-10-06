# Coding Standards

## Language

- Name domain concepts with the terms of the server's glossaries, `../mvdmio-suite/Compliance/docs/CONTEXT.md` and
  `../mvdmio-suite/Auth/docs/CONTEXT.md`, capitalised as they write them (Personal token, Agent connection, Account,
  User, Upload link), and testing concepts with this repo's [`CONTEXT.md`](CONTEXT.md): in identifiers, messages, and
  docs alike.

## Code

- Return every failure as a `Failure` (`src/failure.rs`): `Failure::report` prints it as JSON and picks its exit code.
  `src/` has no `unwrap`, and each `expect` states the invariant that makes it safe (`"the file sits in a folder"`):
  a panic would print plain text and exit 101.
- Write a failure message as sentences for the Agent that reads stderr, ending with the fix when the user has one:
  "No credential. Run `compliance login`, or set COMPLIANCE_TOKEN to a Personal token."
- Mark a discarded result (`let _ =`) with `// Ignored:` and why dropping it is safe.
- Wrap comments at 120 columns; rustfmt wraps only the code, at 100.

## Tests

- Test through the built binary, from `tests/`, against the Fake hosts in `tests/support`: arguments and environment
  go in, and assertions read stdout, stderr, the exit code, and the requests the Fake hosts recorded. `src/` holds no
  `#[cfg(test)]` module.
- Gate a test that runs `login` without `--device` off macOS and Windows with `#[cfg]`: they open a real browser
  whatever the environment, while on Linux `tests/support` removes the display.
- Change `tests/e2e/operations.rs` with any CLI rule it copies; its comments name each source, down to the hand-written
  commands without actions in `src/cli.rs`. `cargo test` checks the copies against the fixture only.

## Docs

- Describe a change to what a command does in `README.md` and `SKILL.md` in the same commit. `SKILL.md` ships inside
  the binary as `compliance skill`, so it describes the build it came with.

## Dependencies and releases

- Keep TLS on rustls: `cargo tree -i openssl-sys` must find nothing.
- Keep the package name `compliance-cli`: dist names the installers after it, and Compliance's `/cli/install.sh` and
  `/cli/install.ps1` redirect to `compliance-cli-installer.sh` and `.ps1` on this repo's latest GitHub Release.
