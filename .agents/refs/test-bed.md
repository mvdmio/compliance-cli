# Test-bed and E2E suite

[`CONTEXT.md`](../../CONTEXT.md) defines the Test-bed, the Launcher, the E2E suite, its Scenarios, and the Sweeps.

## Start a Test-bed

`scripts/test-bed.sh` is the Launcher. It builds Compliance from the `mvdmio-suite` checkout and starts it with its own
throwaway Postgres container and data folder. It needs Docker, the .NET SDK, `bun`, and `curl`, on Linux or macOS.

- It prints one JSON line `{"url": "...", "token": "..."}` on stdout and runs in the foreground. Put the two values in
  `COMPLIANCE_URL` and `COMPLIANCE_TOKEN` to run commands against it. Several can run side by side.
- Ctrl-C or SIGTERM stops Compliance, removes the container, and deletes the folder.
- Every boot reseeds the database, so the Launcher then writes three rows for seed user 1 on Account 1: a Legal
  Acceptance, a Personal token, and an Assistant allowance with 100 USD of extra funds for `chat send`.
- `MVDMIO_SUITE_DIR`: the `mvdmio-suite` checkout it builds, by default `../mvdmio-suite` next to this repo. Set it in
  a worktree, where that default path does not exist.
- `TEST_BED_BOOT_TIMEOUT`: the seconds it allows for the build lock, the build, and the boot together (default 1800).

## Run the E2E suite

`cargo test --test e2e -- --ignored`. To run it on a Test-bed already up, set `COMPLIANCE_E2E_URL` and
`COMPLIANCE_E2E_TOKEN` to its two values; with both unset, it runs the Launcher itself and stops it when it ends.

`tests/e2e/`:

- `main.rs`: the `e2e` test target, which shares `tests/support`.
- `smoke.rs`: the smoke Scenarios.
- `sweeps.rs`: the Help sweep and the Read sweep.
- `operations.rs`: the operations in the live description and the commands the CLI names them by.
- `test_bed.rs`: finding or starting the Test-bed and running the binary on it.
- `timing.rs`: the timing report under `target/e2e/`.

## Nightly run

`.github/workflows/e2e.yml` runs the E2E suite nightly and on manual dispatch. It needs the `MVDMIO_SUITE_DEPLOY_KEY`
secret: the private half of the read-only deploy key "compliance-cli E2E (read-only)" on `mvdmio-suite`, set up once
by a person. GitHub turns the schedule off after 60 days without repository activity; re-enable it in the Actions tab.
