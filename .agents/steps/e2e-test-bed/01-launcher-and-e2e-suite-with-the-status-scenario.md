# 01 — Launcher and E2E suite with the status Scenario

Status: pending

## What to build

The first complete path from "no Test-bed" to "the real `compliance` binary ran against a real Compliance and the
run left a timing report".

**The Launcher** is `scripts/test-bed.sh`, a shell script for Linux and macOS (no Windows). Run it and it:

1. Finds `mvdmio-suite` through `MVDMIO_SUITE_DIR`, defaulting to `../mvdmio-suite` next to this repo's root. It
   fails with a clear stderr message when the folder is missing. (This worktree is not next to `mvdmio-suite`, so
   while working here set `MVDMIO_SUITE_DIR=/data/projects/mvdmio/mvdmio-suite`.)
2. Starts a throwaway Postgres container, image `postgres:18.1` (the tag `mvdmio-suite`'s `TestPostgres.Image`
   uses), under a container name of its own and a free host port. It never touches `mvdmio-dev-postgres`.
3. Boots Compliance (`Compliance/src/mvdmio.Compliance.Web`) with the ordinary Development `dotnet run` on a free
   port, with these configuration overrides (environment variables or command-line configuration, both work):
   - `DbConnection` to its own database,
   - `Data:Path` to its own data folder (a fresh temp folder),
   - `Compliance:PublicBaseUrl` to its own `http://localhost:<port>`,
   - `IdentityServer:BaseUrl` to a local address (Development otherwise names `https://auth.mvdm.io`),
   - `Email:Enabled=false`,
   - `Assistant:UseScriptedModelClient=true`.

   Stripe test keys and the TranslationTools client stay as `mvdmio-suite` commits them.
4. Waits until `GET /openapi/v1.json` answers. The first boot builds the solution, so the wait is minutes, not
   seconds. It gives up after a generous limit with a stderr message and a non-zero exit, cleaning up as in 7.
5. Inserts, for seed user 1 on account 1, the two rows every Development boot wipes:
   - a Legal Acceptance row in `compliance.legal_acceptances` (`account_id`, `accepted_by_user_id`),
   - a Personal token row in `auth.personal_tokens`: the secret is `mvdm_pat_` followed by 32 random bytes in
     base64url without padding; the row stores `secret_hash` = SHA-256 of the secret's UTF-8 bytes, with
     `user_id` 1, `account_id` 1, a name, and no expiry. The shape comes from `mvdmio-suite`'s
     `PersonalTokenSecret` and the two migrations named in the Footprint. Auth does not run.
6. Prints exactly one line to stdout, a JSON object `{"url": "...", "token": "..."}`. Nothing else ever goes to
   stdout: build and server logs go to stderr or to a log file in its data folder.
7. Stays in the foreground. On Ctrl-C or SIGTERM, and on any failure after step 2, it stops Compliance, removes the
   container, and deletes its data folder.

Several Launchers run side by side: every name, port, and folder is its own. Two Launchers started at once from the
same `mvdmio-suite` checkout must not break each other's build. If a plain `dotnet run` cannot promise that, the
Launcher serialises the build (a lock) and then runs with `--no-build`.

**The E2E suite** is the Cargo test target `e2e` (`tests/e2e/main.rs` plus modules). Every test in it is
`#[ignore]`, so `cargo test` lists the Scenarios as ignored and runs none; `cargo test --test e2e -- --ignored`
runs them.

- It takes its Test-bed from `COMPLIANCE_E2E_URL` and `COMPLIANCE_E2E_TOKEN`. When both are unset, it runs the
  Launcher once per test process, reads its first stdout line, and shares that Test-bed with every Scenario. The
  Launcher it started stops and cleans up when the test process ends, including after a failed or aborted run (for
  example, the Launcher watches its parent process, or a pipe the suite holds open, when the suite asks it to). A
  developer's own foreground Launcher is unaffected. One variable set without the other is a clear failure.
- Scenarios run one at a time even without `--test-threads=1`: each Scenario holds a process-wide lock.
- A Scenario runs the built binary only through `COMPLIANCE_URL` and `COMPLIANCE_TOKEN`, with every other
  `COMPLIANCE_*` variable removed and no display. It uses one fresh config and cache folder for the whole Scenario,
  so the API description is fetched once per Scenario, not once per command. It checks stdout, stderr, and the exit
  code only. The Fake-host helpers in `tests/support` (`command`, `Run`) are the prior art; reuse them where they
  fit.
- **Timing report**: every command a Scenario runs is recorded with its Scenario name, its arguments, its exit
  code, and its wall-clock time in milliseconds. The report is a JSON file under the Cargo target folder's `e2e/`
  (`target/e2e/` by default), one file per run, named by the run's start time. It is rewritten after each command,
  so an aborted run keeps what ran. No timing ever fails a run.

**The `status` Scenario**: `compliance status` against the Test-bed exits 0 and prints JSON naming the host, the
credential as `COMPLIANCE_TOKEN`, the seeded User, and Account 1.

**Docs**: `AGENTS.md` gains the Launcher command and the E2E suite command, the variables `MVDMIO_SUITE_DIR`,
`COMPLIANCE_E2E_URL`, and `COMPLIANCE_E2E_TOKEN`, the note that `cargo test` never runs the E2E suite (and needs no
.NET, Docker, or Postgres), and Layout lines for `scripts/test-bed.sh` and `tests/e2e/`.

## Footprint

Projects: compliance-cli (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`)

- `scripts/test-bed.sh` — new: the Launcher
- `tests/e2e/main.rs` — new: the `e2e` target, the `status` Scenario
- `tests/e2e/test_bed.rs` — new: finding or starting the Test-bed, the per-Scenario run helper and lock
- `tests/e2e/timing.rs` — new: the timing report
- `tests/support/mod.rs` — `command`, `Run`: reuse; touch only if reuse needs it
- `Cargo.toml` — only if the target needs an explicit `[[test]]` entry
- `AGENTS.md` — Commands, Layout, the Test-bed variables
- `src/status.rs` — read-only: the fields `status` promises
- `/data/projects/mvdmio/mvdmio-suite/Libraries/test/mvdmio.UnitTest.Common.Postgres/TestPostgres.cs` — read-only: `Image`
- `/data/projects/mvdmio/mvdmio-suite/Auth/src/mvdmio.Auth.Db/PersonalTokenSecret.cs` — read-only: secret and hash shape
- `/data/projects/mvdmio/mvdmio-suite/Auth/src/mvdmio.Auth.Db/Migrations/_202609242057_AddPersonalTokens.cs` — read-only: `auth.personal_tokens`
- `/data/projects/mvdmio/mvdmio-suite/Compliance/src/mvdmio.Compliance.Db/Migrations/_202608282212_LegalAcceptances.cs` — read-only: `compliance.legal_acceptances`
- `/data/projects/mvdmio/mvdmio-suite/Compliance/src/mvdmio.Compliance.Web/appsettings.Development.json` — read-only: the keys the Launcher overrides

## Acceptance criteria

- [ ] `scripts/test-bed.sh` prints exactly one JSON line with `url` and `token` on stdout, and
      `COMPLIANCE_URL=<url> COMPLIANCE_TOKEN=<token> compliance status` then exits 0.
- [ ] Ctrl-C or SIGTERM to the Launcher leaves no container, no Compliance process, and no data folder behind.
- [ ] Two Launchers run at the same time without colliding, and neither touches `mvdmio-dev-postgres`.
- [ ] The Test-bed's Protected Resource Metadata and configuration name no production Auth address.
- [ ] `cargo test` lists the `status` Scenario as ignored and needs no .NET, Docker, or Postgres.
- [ ] `cargo test --test e2e -- --ignored` with no `COMPLIANCE_E2E_*` set starts a Test-bed, passes the `status`
      Scenario, and leaves no Test-bed behind when it ends.
- [ ] With `COMPLIANCE_E2E_URL` and `COMPLIANCE_E2E_TOKEN` pointing at a running Launcher, the same command passes
      without starting a second Test-bed.
- [ ] A run writes a timing report under `target/e2e/` with one entry per command: Scenario, arguments, exit code,
      milliseconds.
- [ ] `AGENTS.md` documents the Launcher, the E2E suite command, the three variables, and that `cargo test` never
      runs the E2E suite.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
