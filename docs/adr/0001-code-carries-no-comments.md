# Code carries no comments

`cargo test` fails on any comment in a Rust file under `src/` or `tests/` (`tests/no_comments.rs`), the E2E suite's files included. Code explains itself through names and structure. A reason that a name cannot carry goes into an ADR, or into the data it explains, and the history of a line lives in git. A comment drifts from the code it describes, and no build or test notices when it does.

The rule comes from mvdmio-suite: [#1433](https://github.com/michielvandermeer/mvdmio-suite/issues/1433) and its ADR-0038, "Code carries no comments". This repo applies it to Rust with fewer exceptions than the suite.

Every comment kind is refused: `//`, `/* */`, `///`, `//!`, `/** */`, and `/*! */`. YAML, TOML, and Markdown files keep their comments, as they do in the suite.

## Differences from mvdmio-suite

- **No doc-comment exception.** The suite keeps `///` on the public members of projects that publish their documentation, because their users read it in IntelliSense. This crate is binary-only (one `[[bin]]` named `compliance`, no `[lib]`), is not published to crates.io, and is installed from GitHub releases through cargo-dist. No outside reader sees its `///` docs.
- **Help text lives in clap attributes.** A `///` on a clap-derived item is the item's help text, so a ban on `///` would silently drop it. The text moves into `#[command(about = "…")]`, `#[command(long_about = "…")]`, and `#[arg(help = "…")]`, and `--help` prints what it printed before.
- **A lint exception carries its reason in the attribute.** The suite allows a reason at the end of a `#pragma warning disable` line. Rust has the reason inside the attribute itself: `#[allow(lint, reason = "…")]` or `#[expect(lint, reason = "…")]`. That is not a comment.
- **No `pre-review:` waiver.** The suite allows it because its pre-review check reads it. No tool in this repo reads one.
- **A `cargo test` check.** The suite fails the build through a compiler analyzer. Stable Rust has no way to add a compiler lint, so the check is a test, `tests/no_comments.rs`. It lexes every `.rs` file with `ra_ap_rustc_lexer`, the lexer rustc and rust-analyzer use, so a `//` inside a string, a raw string, or a char literal does not count. CI already runs `cargo test` on every push and pull request, on Linux, macOS, and Windows.

## The Write sweep table

`tests/e2e/write_table.rs` holds what the API description cannot say about a write, one entry per operation id. Its notes used to explain why an entry is special, for example why a field is fixed or why a write expects a 409. Those reasons are data now, and a note never returns:

- An entry's reason lives in the entry's own `reason`, `skip`, or `no_read` text. `reason` is for an entry that runs but is special. `skip` says why a write does not run. `no_read` says why what it changed cannot be read back.
- An entry goes in the table only where the description's body is wrong. A write the description alone can drive has no entry.
- When a write fails in a Write sweep, the report's `failed:` line for it ends with ` Table entry: ` and the entry's `reason`, so the person reading a red nightly run sees why the entry exists next to what went wrong. A write whose entry has no `reason` prints the plain line.

## Considered Options

- **A `build.rs` check.** Rejected because a build script fails every build and every release build, and it cannot point at the offending line the way a test failure message does. It would also run for people who only install the crate.
- **A dylint (or other custom clippy) lint.** Rejected because it needs a pinned nightly toolchain and a separate tool in CI, where a dev-dependency and one integration test do the same job on stable.
- **Allow `///` on clap-derived items.** Rejected because it makes the rule a list of exceptions that a reader and the check must both know. Moving the text into attributes costs a few lines and keeps the rule free of exceptions.
- **Delete the table's notes outright.** Rejected because about 70 of them are the only record of why an entry is special, for example that the Test-bed's License is complimentary and has no Stripe key. Without them the next developer who changes an entry has only git history to go on, and a red nightly run shows the failure with no hint of the known special case.
- **Move the notes into a Markdown file.** Rejected because a file beside the table is the same drift as a comment: nothing checks it against the entry it describes, and it is not in front of the person reading the failure. A `reason` sits on the entry, and the report prints it.

## Consequences

- The change that adds the rule deletes every existing comment: 481 `///` lines, 48 `//!` lines, and 185 `//` lines. To find a deleted reason, run `git log -p -- <file>` or `git log -S` with its text.
- The comment examples in `tests/no_comments.rs` sit inside string literals, so the check does not report itself.
