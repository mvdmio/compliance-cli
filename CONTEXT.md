# mvdm CLI

The `mvdm` command-line tool for the REST APIs of the mvdm.io Products, and the tests that prove it works against them.

## Language

### The CLI

**Product**:
An mvdm.io service whose REST API the CLI offers. Auth lists the Products at `/agent-access/apps`; an `MVDM_<NAME>_URL` variable can also name one, and wins over the list. Each Product's commands sit under its name: `mvdm <product> …`.
_Avoid_: App, service, host

### Testing against Compliance

**Fake host**:
A stand-in for Auth or a Product that a test starts inside its own process and scripts request by request. It is used by every test that runs on every change.
_Avoid_: Mock server, stub

**Test-bed**:
A real Compliance, built from source and running on the developer's or CI's own machine with its own throwaway database, together with a Personal token for a seeded user. It stays up until someone stops it, so people and Agents can run commands against it by hand.
_Avoid_: Local stack, E2E environment, sandbox

**Launcher**:
The one command that starts a Test-bed and reports its address and Personal token.
_Avoid_: Bootstrap, setup script

**E2E suite**:
The set of Scenarios that runs the real `mvdm` binary, as `mvdm compliance …`, against a Test-bed. It runs only when someone asks for it, or on the nightly schedule, never as part of the ordinary tests.
_Avoid_: Integration tests, system tests

**Scenario**:
One named case in the E2E suite: the CLI commands it runs against a Test-bed and the outcome it expects.
_Avoid_: Test case, flow

**Sweep**:
A Scenario that runs the same check over every generated command the live API description yields, rather than one hand-picked command. The **Help sweep** asks each command for its help, and the **Read sweep** runs each read operation.
_Avoid_: Crawl, fuzz
