# Releases

`dist` (cargo-dist) builds releases from `dist-workspace.toml`; `dist plan` shows what a release builds. Change the
build by the rules in [`CODING_STANDARDS.md`](../../CODING_STANDARDS.md#releases).

## Cut a release

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and commit.
2. Tag the commit `v<version>`, such as `v0.2.0`.
3. Push the commit, then the tag: `git push origin v<version>`. The tag push starts the release workflow, which
   builds every target and publishes a GitHub Release with the shell and PowerShell installers.
