# compliance

`compliance` is a command-line tool for the [Compliance](https://compliance.mvdm.io) REST API. It is built for
people who script Compliance from a terminal, and for their Agents (Claude Code, Codex, Gemini CLI, and others)
that run shell commands. Every command prints JSON, and the exit code says what happened.

The CLI talks to the Compliance REST API only. It collects no usage data and sends none.

## Install

Linux and macOS:

```sh
curl -LsSf https://compliance.mvdm.io/cli/install.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://compliance.mvdm.io/cli/install.ps1 | iex"
```

Run the install line again to upgrade. `compliance --version` shows the build you run.

## Sign in

Set `COMPLIANCE_TOKEN` to a Personal token. You create Personal tokens on the API & Agent Access page of your
account; the CLI never creates them.

```sh
export COMPLIANCE_TOKEN=...
compliance api GET /api/v1/risks
```

## Raw calls

`compliance api <method> <path>` makes one call with your credential. The path is relative to the host, and a
query string rides in it.

```sh
compliance api GET '/api/v1/risks?$top=5'
compliance api POST /api/v1/risks --body '{"title":"Flood"}'
compliance api PATCH /api/v1/risks/r1 --body @risk.json
compliance api GET /api/v1/reports/q3 --out q3.pdf
```

- `--body <json|@file>` sends a JSON body, inline or read from a file.
- `--out <path>` writes the response body to a file and prints `{"path","bytes","contentType","fileName"}`. A
  response that is not JSON needs `--out`: the CLI never writes raw bytes to stdout.

## Output

- stdout holds the JSON result: pretty-printed in a terminal, compact otherwise. An empty success prints
  `{"ok":true,"status":<code>}`.
- stderr holds errors: the API's ProblemDetails as it came, or `{"error":"<code>","message":"…"}` for a failure
  on your side, such as `not-signed-in`, `network`, `file`, or `binary-response`.
- On a rate limit (429) the CLI waits for `Retry-After` and retries, up to three times.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | Success. |
| 1 | Error: see stderr. |
| 2 | Usage mistake: fix the command line. |
| 3 | Browser handoff: a person must finish the act in the browser. |

A Browser handoff prints `{"status":"browser_handoff","reason":…,"handoffUrl":…}` on stdout. Show the link to a
person; the CLI never opens a browser for it.

## Environment

| Variable | Meaning |
| -------- | ------- |
| `COMPLIANCE_TOKEN` | A Personal token, sent as the bearer. |
| `COMPLIANCE_URL` | The Compliance host. Defaults to `https://compliance.mvdm.io`. |

## Licence

MIT. See [LICENSE](LICENSE).
