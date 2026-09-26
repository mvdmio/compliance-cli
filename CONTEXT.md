# Compliance CLI

The `compliance` command-line tool for the Compliance REST API, and the tests that prove it works against Compliance.

## Language

### Testing against Compliance

**Fake host**:
A stand-in for Compliance or Auth that a test starts inside its own process and scripts request by request. It is used by every test that runs on every change.
_Avoid_: Mock server, stub

**Test-bed**:
A real Compliance, built from source and running on the developer's or CI's own machine with its own throwaway database, together with a Personal token for a seeded user. It stays up until someone stops it, so people and Agents can run commands against it by hand.
_Avoid_: Local stack, E2E environment, sandbox

**Launcher**:
The one command that starts a Test-bed and reports its address and Personal token.
_Avoid_: Bootstrap, setup script

**E2E suite**:
The set of Scenarios that runs the real `compliance` binary against a Test-bed. It runs only when someone asks for it, or on the nightly schedule, never as part of the ordinary tests.
_Avoid_: Integration tests, system tests

**Scenario**:
One named case in the E2E suite: the CLI commands it runs against a Test-bed and the outcome it expects.
_Avoid_: Test case, flow

**Sweep**:
A Scenario that runs the same check over every generated command the live API description yields, rather than one hand-picked command. The **Help sweep** asks each command for its help, and the **Read sweep** runs each read operation.
_Avoid_: Crawl, fuzz
