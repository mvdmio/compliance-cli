# Idea — Reproduce production problems on the local E2E test-bed

Status: idea

## Motivation

A user sometimes hits a problem in production that the CLI's fake-host tests cannot show. The planned E2E test-bed runs the real `compliance` binary against a Compliance started on a developer's machine. So it can also rebuild the state a production user was in, and run the same commands that failed for them.

## Goal

When a real production report arrives, a developer or an Agent writes a named triage case that rebuilds the user's state on the local Compliance and runs the commands that failed. Once fixed, the case stays as a permanent scenario in the E2E suite.

## Decisions (locked)

- Production data never leaves production. The state is rebuilt by hand from the report, through the CLI and the API, with SQL only where the API cannot reach.
- This waits for a real scenario to triage. The first E2E test-bed only lays the groundwork.

## Out of scope

- Restoring or anonymising a production database snapshot.
- Pointing the CLI at production. `COMPLIANCE_URL` already does that.

## Open questions

- What does a triage case need that an ordinary E2E scenario does not? For example: several Accounts, a specific License state, a large register, or a particular Upload history.
- Does a triage case live beside the E2E scenarios, or in a folder of its own?
