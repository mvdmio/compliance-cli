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

## Commands from the API

Every REST operation is a command. The CLI builds these commands when it runs, from the host's OpenAPI description
at `/openapi/v1.json`, so a new operation shows up without a new CLI release.

```sh
compliance risks list --filter "status eq 'Open'" --top 5
compliance risks accept 3f2c9a
compliance risks create --title Flood --likelihood 3
compliance evidence download 8d1e --out scan.pdf
```

- **Names.** An operation's `operationId` `<group>.<action>` becomes `compliance <group> <action>`, both in
  kebab-case. So `risks.accept` is `compliance risks accept`. The group comes from the `operationId`, not from the
  tag: `compliance <group> --help` lists the tags its actions carry.
- **Arguments.** Path parameters are positional arguments, in path order. Query parameters are options, with the
  OData `$` dropped: `--filter`, `--select`, `--orderby`, `--expand`, `--top`, `--skip`. The top-level fields of a
  JSON body are options too, kebab-cased (`collectedAt` is `--collected-at`) and typed from the schema: numbers
  are sent as numbers, `true`/`false` as booleans, a list option repeats, and an object takes JSON text.
- **`--body <json|@file>`** sends a whole body. Field options win over the same keys in it. An operation that
  needs a body and gets none sends `{}`.
- **Name clashes.** `--help`, `--body`, `--out`, and `--file` are always the CLI's own. After them, query
  parameters take their names before body fields. A name already taken gets a `query-` or `field-` prefix (so a
  body field `out` is `--field-out`), and the option's help says so.
- **Downloads.** An operation that answers a file, not JSON, needs `--out <path>`. Without it, the command is a
  usage mistake and sends nothing.
- **Help.** `compliance --help` lists the hand-written commands and the groups. `compliance <group> --help` lists
  its actions, and `compliance <group> <action> --help` shows the operation's summary, its description, and every
  argument with its type.

The hand-written commands (`login`, `logout`, `status`, `accounts list`, `accounts switch`, `chat send`, `api`,
and `--version`) never need the description, and win over a generated command of the same name. Other actions in the
same group, such as a generated `accounts get`, stay reachable. A hand-written command without actions, such as `status`, hides a generated
group of the same name.

### Files

An operation that takes an upload id, such as `evidence create --upload-id`, also takes `--file <path>`. The CLI
sends the file first and then makes the call with the new upload id. `--file` and the upload id option cannot be
given together.

```sh
compliance evidence create --file scan.pdf --title "Scan"
compliance conversations send-message 12 --text "See these" --file a.pdf --file b.png
```

- **Lists.** Where the operation takes a list of upload ids, such as `uploadIds`, `--file` repeats. Each file
  becomes one id, in order. A file that cannot be read fails with `file` before any file is sent.
- **Parts.** Each file goes through its own Upload link. A file up to the link's part size goes in one request; a
  larger one, such as an Import ZIP, goes in parts of that size and then a finish. The part size always comes
  from the link. A file larger than the link takes fails with `file` before any bytes go.
- **Resume.** When a part is lost, or the link answers that it holds a different number of bytes, the CLI asks
  the link how many bytes it holds and goes on from there. After five tries in a row that move no byte forward,
  it fails with `upload`.
- **The credential stays home.** The Upload link needs no bearer, so the CLI sends none to it.
- stdout holds only the result of the final call. The CLI prints no upload progress.

### The cached description

The CLI keeps a copy of the description per host, in the `compliance` folder of your cache folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CACHE_HOME/compliance` or `~/.cache/compliance` |
| macOS | `~/Library/Caches/compliance` |
| Windows | `%LOCALAPPDATA%\compliance` |

`COMPLIANCE_CACHE_DIR` sets another folder. A copy younger than one hour is used as it is. An older one is
fetched again; if that fails, the old copy is used. With no copy and no answer from the host, a generated command
fails with `network`.

A new operation reaches you by itself. When you type a group or action the copy does not know, the CLI fetches a
fresh copy at once, even within the hour. If the command is still unknown, it is a usage mistake (exit 2).

## Talk to our Assistant

```sh
compliance chat send "Which risks are still open?"
```

`chat send "<text>"` sends one message to our Assistant, as a person does in the Assistant panel, and waits
until its turn ends. Without `--conversation <id>`, it starts a new conversation. It then prints:

```json
{"conversationId":"…","turnState":"idle","lastError":null,"queued":false,"messages":[…]}
```

- `messages` holds every message after the one you sent, oldest first, as the API gives it: `seq`, `role`,
  `sender`, the markdown `text`, `attachments`, and `toolActivity` (the tools our Assistant ran, each with its
  outcome).
- `conversationId` is the conversation your message landed in. When our Assistant judges it a new topic, this is
  a new conversation, not the one you named. Continue with that id.
- `lastError` says why the turn stopped early, when it did.
- `queued` is true when your message went into a shared conversation while another member's turn was running.
  Our Assistant answers it in the turn after that one, so `messages` may hold only that member's reply. Read the
  answer later with `compliance conversations list-messages <conversationId> --after <seq> --wait 60`.

**Answer a question or proposal.** Our Assistant asks and proposes in the text of its messages. Answer with the
next `chat send`, passing the printed `conversationId`:

```sh
compliance chat send "Yes, go ahead." --conversation <conversationId>
```

An empty Assistant allowance (402 `allowance-exhausted`) or a conversation that cannot take a message now (409)
prints the ProblemDetails on stderr and exits 1. For everything else about conversations, such as listing them or
stopping a turn, use the generated `compliance conversations …` commands.

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
  on your side, such as `not-signed-in`, `network`, `file`, `upload`, or `binary-response`.
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
| `COMPLIANCE_CACHE_DIR` | The folder for the cached API description. |

## Licence

MIT. See [LICENSE](LICENSE).
