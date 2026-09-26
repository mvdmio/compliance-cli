# E2E test-bed: run the CLI against a locally started Compliance

Status: ready-for-agent
Blocked by: mvdmio-suite `dev-boot-email-off-and-scripted-assistant`

## Problem Statement

Every CLI test today runs against Fake hosts: stand-ins for Compliance and Auth that the test starts and scripts itself. The fakes are fast and run on every change. But they only know what the tests tell them, and the fixture API description is a hand-written subset of 12 operations. The real Compliance has 403.

So a Compliance change can break the CLI without any test noticing, and so can a CLI change that only works against the fakes. No one finds out until a user hits it. There is also no ready place to measure whether a change made the CLI or Compliance faster, or to rebuild a production problem locally.

## Solution

A Test-bed: a real Compliance, built from the `mvdmio-suite` source and running on the developer's or CI's own machine with its own throwaway database, together with a Personal token for a seeded user.

- **The Launcher** is one command. It starts a Test-bed, prints its address and Personal token as one JSON line, and keeps it running until stopped. Developers and Agents use it to run commands by hand.
- **The E2E suite** is a set of Scenarios that run the real `compliance` binary against a Test-bed. It covers one smoke Scenario per CLI surface, plus a Help sweep over every generated command and a Read sweep over every read operation. It runs only when asked, and nightly.
- **Timing.** Every E2E suite run writes a timing report, so two runs can be compared before and after a speed change.
- **Nightly.** A scheduled workflow in this repo builds the CLI from HEAD and Compliance from `mvdmio-suite` `master`, and runs the E2E suite.

The ordinary tests stay as they are: `cargo test` still needs no .NET, no Docker, and no Postgres.

## User Stories

1. As a developer, I want one command that starts a real Compliance on my machine, so that I can run the CLI against it without setting anything up by hand.
2. As a developer, I want the Launcher to print the Test-bed's address and Personal token as one JSON line, so that I can paste them into `COMPLIANCE_URL` and `COMPLIANCE_TOKEN`.
3. As an Agent working in this repo, I want to run the Launcher in the background and read its first line, so that I can drive the CLI against a real Compliance while I work.
4. As a developer, I want the Test-bed to stop and clean up when I press Ctrl-C, so that no container or server is left running.
5. As a developer, I want the Test-bed to use its own database and data folder, so that it never wipes my ordinary Development data.
6. As a developer with several worktrees, I want to run several Test-beds side by side, so that parallel sessions do not collide.
7. As a developer, I want the Launcher to find `mvdmio-suite` next to this repo by default, and elsewhere through an environment variable, so that it works with my existing checkout.
8. As a developer, I want the seeded user to already have a License (from the seed), a Legal Acceptance, and a Personal token, so that no command hits a Browser handoff or a missing sign-in.
9. As a developer, I want the Test-bed to send no email and call no live language model, so that running it is safe and free.
10. As a developer, I want nothing in the Test-bed to point at production Auth, so that no command reaches production by accident.
11. As a developer, I want to run the E2E suite with one Cargo command, so that I don't learn a new tool.
12. As a developer, I want the E2E suite to start a Test-bed itself when I have none running, so that one command is enough.
13. As a developer, I want the E2E suite to use a Test-bed I already started, so that repeated runs skip the boot.
14. As a developer, I want `cargo test` to stay fast and to need no .NET or Docker, so that ordinary work is not slowed down.
15. As a developer, I want `cargo test` to list the E2E Scenarios as ignored, so that I can see they exist without them running.
16. As a maintainer, I want `status` checked against the real Compliance, so that a change to the accounts answer is caught.
17. As a maintainer, I want `accounts list` and `accounts switch` checked against the real Compliance, so that Account switching keeps working.
18. As a maintainer, I want `api` checked with a GET and with a 404, so that raw requests and problem+json errors keep working.
19. As a maintainer, I want one generated write command with `--body` checked, so that body building works against the real API.
20. As a maintainer, I want `--file` checked with a small file and with a file sent in parts, so that Upload links keep working end to end.
21. As a maintainer, I want `chat send` checked until the conversation is idle, so that the conversation and long-poll flow keeps working.
22. As a maintainer, I want a bad token checked to give exit 1, so that the CLI still reports a refused credential correctly.
23. As a maintainer, I want every generated command to answer `--help`, so that a description change the CLI cannot build from is caught.
24. As a maintainer, I want every read operation in the live description run through its generated command, so that Compliance's reads keep working from the CLI.
25. As a maintainer, I want the Read sweep to fail when the CLI does not offer a command for an operation in the description, so that the CLI silently dropping an operation is caught.
26. As a maintainer, I want the Read sweep to find ids from the Test-bed's own data, so that it works on any data, including a rebuilt production state later.
27. As a maintainer, I want reads under an empty list reported as skipped, not failed, so that a missing seed row does not look like a CLI bug.
28. As a maintainer, I want the two file downloads checked for a non-empty file, so that binary answers keep working.
29. As a developer working on speed, I want every run to write the wall-clock time of each command to a report, so that I can compare a run before and after my change.
30. As a developer, I want timings never to fail a run, so that noisy machines do not produce false failures.
31. As the maintainer, I want the E2E suite to run every night against the newest Compliance, so that drift is caught within a day.
32. As the maintainer, I want a nightly failure to notify me, so that I don't have to check the run.
33. As the maintainer, I want the nightly timing report kept as an artifact, so that I can look at speed over time.
34. As a future triager, I want the Test-bed to hold whatever state a Scenario builds, so that I can later rebuild a production problem on it.

## Implementation Decisions

**The Launcher**
- It is a shell script in this repo, for Linux and macOS only. Windows stays covered by the Fake-host tests.
- It finds `mvdmio-suite` through `MVDMIO_SUITE_DIR`, which defaults to the sibling folder `../mvdmio-suite`.
- It starts a throwaway Postgres container with a name of its own and a free port. It uses Postgres 18, the major version of `mvdmio-suite`'s Testcontainers image. It never uses the developer's `mvdmio-dev-postgres`, because a Development boot truncates and reseeds the database.
- It boots Compliance through the ordinary Development `dotnet run` on a free port, with these configuration overrides:
  - `DbConnection` set to its own database,
  - `Data:Path` set to its own data folder,
  - `Compliance:PublicBaseUrl` set to its own address, because the OAuth resource is derived from it,
  - `IdentityServer:BaseUrl` set to a local address, so the Protected Resource Metadata and Browser handoff links never name production Auth,
  - email off, and the scripted Assistant model on. These are the switches added by the prerequisite Spec.
- It waits until the API description answers. Then it inserts two rows for seed user 1 on account 1, because every boot wipes both:
  - a Legal Acceptance,
  - a Personal token: `mvdm_pat_` followed by 32 random bytes in base64url, stored as the SHA-256 of the secret's UTF-8 bytes.

  Auth does not need to run, because the API checks Personal tokens against the database.
- It then prints exactly one line to stdout, a JSON object with `url` and `token`. It stays in the foreground. On Ctrl-C or SIGTERM it stops Compliance, removes the container, and deletes its data folder.
- Stripe test keys and the TranslationTools client stay as `mvdmio-suite` commits them.

**The E2E suite**
- It is a Cargo test target of its own. Every test in it is marked ignored, so `cargo test` lists them and does not run them. It runs with `cargo test --test e2e -- --ignored`. Format and lint check it like any other code.
- It takes its Test-bed from `COMPLIANCE_E2E_URL` and `COMPLIANCE_E2E_TOKEN`. When those are unset, it runs the Launcher itself, reads the first line, and stops the Launcher when the run ends.
- It runs the built binary only through the CLI's own `COMPLIANCE_URL` and `COMPLIANCE_TOKEN`, with a fresh config and cache folder per Scenario, as the Fake-host tests do. It checks stdout, stderr, and the exit code only.
- It runs its Scenarios one at a time. Every Scenario shares the one Personal token, and `accounts switch` moves that token to another Account for as long as it runs.

**Smoke Scenarios**, one per surface:
- `status`
- `accounts list`, and `accounts switch` to seed Account 2 and back to Account 1. The switch persists on the Personal token, and Account 2 has no Compliance License, so the Scenario checks only the switch answers and always switches back.
- `api` with a GET, and with a 404 that returns problem+json
- one generated write command with `--body`
- `--file` to an Evidence upload, with a small file sent in one request, and a file just over one 25 MiB part, sent in parts. Evidence allows 50 MiB per file. A Conversation upload allows only 25 MiB, so it cannot take the part-sized file.
- `chat send`, until the conversation is idle
- a bad token, which gives exit 1

**Help sweep**
- It runs `--help` on every generated command the live description yields.

**Read sweep**
- It fetches the live `/openapi/v1.json` and takes every GET operation.
- It derives each command name with the CLI's rule: the operationId is split on its first `.` into group and action, and each part is kebab-cased. When `--help` does not list that command, the read fails.
- `accounts.list` runs as the hand-written `accounts list`, which replaces the generated one.
- For a read that needs ids, the sweep first runs the list whose path the read extends, then takes the first item's `id`. A small table covers the cases the paths cannot explain:
  - `check-runs/{id}` takes `checkRunId` from `check-results`,
  - a requirement `{code}` takes `code` from the framework's requirements list,
  - conversation messages take the id from the conversations list.
- When the list a read depends on is empty, as `import-sessions` always is after the seed, the reads under it are reported as skipped.
- A read passes when the command exits 0 and prints valid JSON. A download passes when it writes a non-empty file through `--out` into a temp folder. The sweep does not check the answer against the response schema.

**Timing report**
- Each run writes a JSON report under `target/e2e/`, with one entry per command run: its Scenario, its arguments, its exit code, and its wall-clock time.
- No timing ever fails a run.

**Nightly**
- A scheduled GitHub Actions workflow in this repo runs on `ubuntu-latest`. It installs .NET 10, because `mvdmio-suite` pins no SDK.
- It checks out `mvdmio-suite` `master` with `GIT_LFS_SKIP_SMUDGE=1` and a read-only token secret, `MVDMIO_SUITE_TOKEN`.
- It builds the CLI from HEAD, then runs the E2E suite, which starts the Test-bed through the Launcher. Docker is already on the runner.
- It uploads the timing report as a workflow artifact. A failure notifies through GitHub's default notification for scheduled runs.
- The existing CI workflow is unchanged.

**Docs**
- `AGENTS.md` gains the Launcher and E2E suite commands, the Test-bed's environment variables, and the note that `cargo test` never runs the E2E suite.
- `CONTEXT.md` already defines Fake host, Test-bed, Launcher, E2E suite, Scenario, and Sweep.

## Testing Decisions

- The E2E suite is the test. It drives the binary the way an Agent does: arguments and environment go in, and stdout, stderr, and the exit code come out. It never checks internal types. This follows the repo's rule for every test.
- There is one seam, the built binary against a running Test-bed. The Launcher is not tested on its own. Every E2E suite run that starts a Test-bed exercises it.
- A Scenario checks what the CLI is responsible for: the exit code, valid JSON, and the fields the command promises. It does not check Compliance's business rules. Those belong to `mvdmio-suite`'s own tests.
- Prior art:
  - the Fake-host tests' way of running the binary with a clean environment and reading `Run {code, stdout, stderr}`,
  - `mvdmio-suite`'s `Auth.Tests.E2E`, which runs real Compliance and Auth hosts against a throwaway Postgres. It is opt-in through `RunE2E=true` and runs nightly in the "e2e licensing" workflow on a self-hosted runner,
  - the Personal token and Legal Acceptance rows that `mvdmio-suite`'s integration and E2E tests create through its repositories. The Launcher writes the same rows with SQL.
- The one-time setup of the `MVDMIO_SUITE_TOKEN` secret is done by a person. The workflow's first scheduled or manually started run proves it.

## Out of Scope

- `compliance login`, `logout`, and refresh against the real Auth. They stay covered by the Fake-host tests and by `mvdmio-suite`'s `CliConsentTests`.
- Chat against a live language model. The Assistant is proven in `mvdmio-suite`.
- Checking that the Fake hosts still match the real Compliance.
- Checking answers against the description's response schemas.
- Every write operation. That is the Idea `.agents/ideas/e2e-write-sweep.md`.
- Triage cases rebuilt from production reports. That is the Idea `.agents/ideas/e2e-triage-scenarios.md`. No production data ever leaves production.
- Timing thresholds, baselines, and load Scenarios.
- Windows support for the Launcher.
- The email and scripted model switches in `mvdmio-suite`. They belong to the prerequisite Spec.

## Further Notes

- The prerequisite Spec lives in the other repo, at `mvdmio-suite/.agents/specs/dev-boot-email-off-and-scripted-assistant.md`. It must land on `mvdmio-suite` `master` before nightly can pass.
- Scripted Assistant Turns still debit the seeded Account's Assistant allowance. If `chat send` ever answers 402, the seeded included funds have run out, which is not a CLI failure.
- Compliance issues upload parts of 25 MiB. An Upload link expires 15 minutes after its last accepted part, not 15 minutes after it was issued. The part-sized Evidence upload stays well inside both limits.
- The Read sweep covers 104 GET operations as of 2026-09-26. Its duration is the main cost of a run, and the timing report shows it.
