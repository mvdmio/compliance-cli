# Workflow actions on Node.js 24

Status: ready-for-agent

## Problem Statement

The CI workflow and the E2E workflow use actions that GitHub has marked as built for Node.js 20. GitHub now runs those
actions on Node.js 24 anyway. It adds a warning to every run that names them: `actions/checkout@v4` and
`actions/upload-artifact@v4`. The upload step also prints two Node.js deprecation warnings in its log (DEP0040 about
`punycode` and DEP0169 about `url.parse()`).

Nothing fails today. But a maintainer who opens a run sees a warning on a green run, and has to decide each time
whether it matters. GitHub will also stop forcing old actions onto a new Node.js at some point. When it does, runs can
break with no change in this repo.

E2E run 36244747803 (manual dispatch, 2026-09-26) shows the warning. That run passed all 10 Scenarios.

## Solution

Move the hand-written workflows to the same major versions that the release workflow already uses:
`actions/checkout@v6` and `actions/upload-artifact@v7`. Those versions run on Node.js 24. After the change, CI and E2E
runs show no Node.js 20 warning.

## User Stories

1. As a maintainer, I want a green E2E run to show no warnings, so that a warning I do see is worth reading.
2. As a maintainer, I want a green CI run to show no warnings, so that I can trust the run summary at a glance.
3. As a maintainer, I want every workflow in this repo to use the same major version of each action, so that I do not
   have to remember which workflow is behind.
4. As a maintainer, I want the nightly E2E suite to keep checking out `mvdmio-suite` with the read-only deploy key, so
   that the upgrade does not break access to the server source.
5. As a maintainer, I want the E2E suite to keep uploading the timing report as the `e2e-timing` artifact, so that I
   can still compare how long runs take.
6. As a maintainer, I want the timing report upload to still run when the E2E suite fails, so that I can see timing
   for a failed run.
7. As a maintainer, I want CI to keep running format, lint, and test on Linux, macOS, and Windows, so that the upgrade
   does not narrow what CI checks.
8. As a maintainer, I want the upgrade to be safe from breakage when GitHub stops running Node.js 20 actions, so that
   the nightly run keeps working without my attention.

## Implementation Decisions

- The CI workflow changes one action: `actions/checkout` goes from `v4` to `v6`.
- The E2E workflow changes three uses of two actions. Both `actions/checkout` steps go from `v4` to `v6`: one checks
  out this repo, and one checks out `mvdmio-suite`. The `actions/upload-artifact` step goes from `v4` to `v7`.
- The target versions match the release workflow. That workflow already uses `actions/checkout@v6` and
  `actions/upload-artifact@v7`. Both versions declare Node.js 24 as their runtime, and both still take every input the
  E2E workflow passes.
- The release workflow stays as it is. `dist` generates it from `dist-workspace.toml`, and nobody edits it by hand.
- Every input the E2E workflow passes today stays the same. For the `mvdmio-suite` checkout those inputs are
  `repository`, `ref`, `ssh-key`, `path`, and `lfs`. For the upload they are `name`, `path`, and `if-no-files-found`,
  and the step keeps `if: always()`. Triage confirmed that the new versions still define these inputs. The implementer
  still reads the release notes of each new major version, to catch an input whose meaning changed.
- Other actions stay as they are: `actions/setup-dotnet@v5`, `oven-sh/setup-bun@v2`, and `dtolnay/rust-toolchain`.
  GitHub did not name them in the warning.

## Testing Decisions

- No code in `src/` or `tests/` changes, so no new test is written. The workflows are tested by running them.
- CI is checked by a push of the change. The CI run must pass on all three systems, with no Node.js 20 warning in its
  annotations.
- The E2E workflow is checked by a manual dispatch after the push (`gh workflow run e2e.yml`). That run must pass all
  Scenarios. It must also show the `mvdmio-suite` checkout succeeding with the deploy key and the `e2e-timing`
  artifact uploaded. Its annotations must hold no Node.js 20 warning.
- E2E run 36244747803 is the prior art: the same workflow, before the change, with the warning.

## Out of Scope

- The notice that `ubuntu-latest` moves to Ubuntu 26 from 2026-10-19. That needs no change now. The nightly E2E run
  and CI will show whether the new image breaks anything.
- The release workflow and its `dist` config.
- Pinning actions to commit SHAs in place of version tags.
- The orphan `dotnet` and `VBCSCompiler` processes that GitHub stops at the end of an E2E run. Those are build servers
  that `dotnet build` leaves running on purpose, not the Test-bed's Compliance.

## Further Notes

- The warning links to GitHub's notice:
  https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
