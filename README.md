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

```sh
compliance login
```

`login` opens your browser at Auth. Sign in there and allow the CLI; the terminal then prints
`{"status":"logged_in","host":…,"user":{"name":…,"email":…},"account":{…}}`. While it waits, stderr holds
`{"status":"browser_opened","authorizeUrl":…}`, so you can open the link by hand. It gives up after 5 minutes.

On a machine without a browser, such as a server over SSH, use the device code. `login` falls back to it by
itself when it cannot open a browser.

```sh
compliance login --device
```

stderr then holds `{"status":"device_code","verificationUriComplete":…,"verificationUri":…,"userCode":…,"expiresIn":…}`.
Open the link on any device and sign in there. The CLI waits until you have.

The CLI finds Auth by itself, through the host's Protected Resource Metadata. You only ever set the Compliance
host.

A sign-in is an Agent connection: it acts as you, in one of your Accounts at a time. The CLI renews its access
token by itself. When the connection has ended, commands fail with `not-signed-in`; run `compliance login` again.

```sh
compliance logout
```

`logout` ends the Agent connection at Auth, so it stops working everywhere, and then forgets the sign-in. If Auth
cannot be reached, the sign-in stays, and you can run `logout` again.

### Stored sign-in

The sign-in is stored in `credentials.json`, in the `compliance` folder of your config folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CONFIG_HOME/compliance` or `~/.config/compliance` |
| macOS | `~/Library/Application Support/compliance` |
| Windows | `%APPDATA%\compliance` |

`COMPLIANCE_CONFIG_DIR` sets another folder. The file holds one sign-in per host, so a sign-in on another host
sits beside it. On Linux and macOS, only you can read it (mode 0600).

### Personal token

For unattended jobs, such as CI, set `COMPLIANCE_TOKEN` to a Personal token. You create Personal tokens on the API
& Agent Access page of your account; the CLI never creates them. `COMPLIANCE_TOKEN` always wins over the stored
sign-in, and `logout` leaves it alone.

```sh
export COMPLIANCE_TOKEN=...
compliance api GET /api/v1/risks
```

## Status and Accounts

```sh
compliance status
```

`status` shows who you are and where commands land:
`{"host":…,"signedIn":…,"credential":{"kind":…,"source":…},"user":…,"account":…,"accounts":[…]}`. The credential
kind is `personal-token` or `agent-connection`, and the source is `COMPLIANCE_TOKEN` or `stored sign-in`. Without
a credential it prints `"signedIn": false` and exits 0.

```sh
compliance accounts list
compliance accounts switch 42
```

`accounts list` lists your Accounts, the current one marked `"current": true`. `accounts switch <id>` moves the
credential into another Account, for every later command and for everyone who uses that credential.

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
| `COMPLIANCE_TOKEN` | A Personal token, sent as the bearer. Wins over the stored sign-in. |
| `COMPLIANCE_URL` | The Compliance host. Defaults to `https://compliance.mvdm.io`. |
| `COMPLIANCE_CONFIG_DIR` | The folder for the stored sign-in. |

## Licence

MIT. See [LICENSE](LICENSE).
