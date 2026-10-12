# mvdm

`mvdm` is a command-line tool for the REST APIs of the [mvdm.io](https://auth.mvdm.io) Products. One binary and one
sign-in serve every Product that offers an API. It is built for people who script the Products from a terminal, and
for their Agents (Claude Code, Codex, Gemini CLI, and others) that run shell commands. Every command prints JSON,
and the exit code says what happened.

The CLI talks to Auth and to the Products' REST APIs only. It collects no usage data and sends none: no telemetry,
no crash reports, no update checks.

## Install

Linux and macOS:

```sh
curl -LsSf https://auth.mvdm.io/cli/install.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://auth.mvdm.io/cli/install.ps1 | iex"
```

The installer puts the `mvdm` binary in `~/.cargo/bin` (or `$CARGO_HOME/bin`) and tells you if that folder is not
on your `PATH`. Builds exist for Linux (x64 and arm64, static), macOS (arm64 and x64), and Windows (x64).

Run the install line again to upgrade: the CLI never updates itself. `mvdm --version` shows the build you run.

## Products and commands

A Product is a mvdm.io service whose REST API the CLI offers. The CLI learns which Products exist from a public
list that Auth serves, so a new Product needs no CLI release.

```sh
mvdm --help                  # the Products and the top-level commands
mvdm <product> --help        # that Product's commands, ending with the address of its guides
mvdm <product> <group> --help
```

The layout of commands:

- `mvdm login`, `mvdm logout`, `mvdm status`, `mvdm accounts list`, `mvdm accounts switch <id>`, `mvdm skill`, and
  `mvdm --version` sit at the top level, because one sign-in acts in one Account for every Product.
- Everything else sits under a Product's name: `mvdm <product> <group> <action>`, `mvdm <product> api <method>
  <path>`, and `mvdm <product> chat send` where the Product offers its Assistant.

A top-level command hides a Product of the same name.

## For Agents

```sh
mvdm skill
```

`skill` prints one Markdown skill file that teaches an Agent when and how to use the CLI: signing in, command
names, exit codes, files, a Product's Assistant, and Browser handoffs. It names no Product. It is the only command
whose output is not JSON. Give it to your Agent as a skill, or let the Agent run it. The same file is
[`SKILL.md`](SKILL.md) in this repo.

## Sign in

```sh
mvdm login
```

`login` opens your browser at Auth. Sign in there and allow the CLI; the terminal then prints
`{"status":"logged_in","auth":…,"user":{"name":…,"email":…},"account":{…}}`. While it waits, stderr holds
`{"status":"browser_opened","authorizeUrl":…}`, so you can open the link by hand. It gives up after 5 minutes. You
sign in once: the connection reaches every listed Product.

On a machine without a browser, such as a server over SSH, use the device code. `login` falls back to it by
itself when it cannot open a browser.

```sh
mvdm login --device
```

stderr then holds `{"status":"device_code","verificationUriComplete":…,"verificationUri":…,"userCode":…,"expiresIn":…}`.
Open the link on any device and sign in there. The CLI waits until you have.

The CLI finds Auth at `https://auth.mvdm.io`, or at `MVDM_AUTH_URL`, and reads its endpoints from
`/.well-known/openid-configuration`.

A sign-in is an Agent connection: it acts as you, in one of your Accounts at a time. The CLI renews its access
token by itself. When the connection has ended, commands fail with `not-signed-in`; run `mvdm login` again.

```sh
mvdm logout
```

`logout` ends the Agent connection at Auth, so it stops working everywhere, and then forgets the sign-in. If Auth
cannot be reached, the sign-in stays, and you can run `logout` again.

### Stored sign-in

The sign-in is stored in `credentials.json`, in the `mvdm` folder of your config folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CONFIG_HOME/mvdm` or `~/.config/mvdm` |
| macOS | `~/Library/Application Support/mvdm` |
| Windows | `%APPDATA%\mvdm` |

`MVDM_CONFIG_DIR` sets another folder. The file holds one sign-in per Auth address, so a sign-in at another Auth
sits beside it. On Linux and macOS, only you can read it (mode 0600).

### Personal token

For unattended jobs, such as CI, set `MVDM_TOKEN` to a Personal token. You create Personal tokens on the API &
Agent Access page of your account; the CLI never creates them. `MVDM_TOKEN` always wins over the stored sign-in,
serves every Product, and `logout` leaves it alone.

```sh
export MVDM_TOKEN=...
mvdm <product> api GET /api/v1/risks
```

### Another host for one Product

`MVDM_<NAME>_URL` sets one Product's base URL and wins over Auth's list. `<NAME>` is the Product's command name in
capitals. With it, the Product's other addresses use the default paths: `/openapi/v1.json`, `/api`, and
`/agents` (the CLI does not talk to MCP).
A Product named only by such a variable works without Auth, with a Personal token. A failed fetch of Auth's list
then fails only a command for a Product no variable names. This is how a developer runs the CLI against
a local build.

## Status and Accounts

```sh
mvdm status
```

`status` shows who you are and where commands land:
`{"auth":…,"signedIn":…,"credential":{"kind":…,"source":…},"user":…,"account":…}`. The credential kind is
`personal-token` or `agent-connection`, and the source is `MVDM_TOKEN` or `stored sign-in`. `user` comes from the
sign-in, so it is `null` with a Personal token: the API has no call that names the User. Without a credential it
prints `"signedIn": false` and exits 0.

```sh
mvdm accounts list
mvdm accounts switch 42
```

`accounts list` lists your Accounts, the current one marked `"current": true`. `accounts switch <id>` moves the
credential into another Account, for every later command in every Product and for everyone who uses that
credential.

`status` and `accounts` call the Account operations `accounts.list` and `accounts.switch` of the first Product a
`MVDM_<NAME>_URL` variable names, in name order, or else of the first listed Product. Every Product hosts the same
Account operations, and they skip the License check, so the first Product serves you whatever your Licenses.

## Commands from the API

Every REST operation of a Product is a command. The CLI builds these commands when it runs, from the Product's
OpenAPI description (`/openapi/v1.json` by default), so a new operation shows up without a new CLI release.

```sh
mvdm <product> risks list --filter "status eq 'Open'" --top 5
mvdm <product> risks accept 3f2c9a
mvdm <product> risks create --title Flood --likelihood 3
mvdm <product> evidence download 8d1e --out scan.pdf
```

- **Names.** An operation's `operationId` `<group>.<action>` becomes `mvdm <product> <group> <action>`, both in
  kebab-case. So `risks.accept` is `mvdm <product> risks accept`. The group comes from the `operationId`, not from
  the tag: `mvdm <product> <group> --help` lists the tags its actions carry.
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
  usage mistake and sends nothing. Only these operations have `--out`.
- **Help.** `mvdm --help` lists the Products. `mvdm <product> --help` lists the hand-written commands and the
  groups of that Product, and ends with `Docs: <its guides address>`. `mvdm <product> <group> --help` lists its
  actions, and `mvdm <product> <group> <action> --help` shows the operation's summary, its description, and every
  argument with its type.

Inside a Product, the hand-written commands (`api`, and `chat send` where the Product offers it) win over a
generated command of the same name. Other actions in the same group stay reachable.

### Files

An operation that takes an upload id, and that sits in a Product whose description holds `uploads.create`, also
takes `--file <path>`. The CLI sends the file first and then makes the call with the new upload id. `--file` and the
upload id option cannot be given together.

```sh
mvdm <product> evidence create --file scan.pdf --title "Scan"
mvdm <product> conversations send-message 12 --text "See these" --file a.pdf --file b.png
```

- **Lists.** Where the operation takes a list of upload ids, such as `uploadIds`, `--file` repeats. Each file
  becomes one id, in order. A file that cannot be read fails with `file` before any file is sent.
- **Parts.** Each file goes through its own Upload link. A file up to the link's part size goes in one request; a
  larger one, such as an Import ZIP, goes in parts of that size and then a finish. The part size always comes
  from the link. A file larger than the link takes fails with `file` before any bytes go.
- **Resume.** When the link answers 409 with the number of bytes it holds, the CLI goes on from there. When a
  request gets no usable answer, the CLI asks the link how many bytes it holds and goes on from there. A file
  whose one request failed goes on in parts, since the link takes a whole file only while it holds no byte.
  After five tries in a row that move no byte forward, it fails with `upload`.
- **The credential stays home.** The Upload link needs no bearer, so the CLI sends none to it.
- stdout holds only the result of the final call. The CLI prints no upload progress.

### The cached lists

The CLI keeps a copy of Auth's Product list per Auth address, and of each Product's description per Product address, in the
`mvdm` folder of your cache folder:

| OS | Folder |
| -- | ------ |
| Linux | `$XDG_CACHE_HOME/mvdm` or `~/.cache/mvdm` |
| macOS | `~/Library/Caches/mvdm` |
| Windows | `%LOCALAPPDATA%\mvdm` |

`MVDM_CACHE_DIR` sets another folder. A copy younger than one hour is used as it is. An older one is fetched again;
if that fails, the old copy is used. With no copy and no answer, a command that needs it fails with `discovery`
(the Product list) or `network` (a description).

A new Product or operation reaches you by itself. When you type a Product, group, or action the copy does not know,
the CLI fetches a fresh copy at once, even within the hour. If the command is still unknown, it is a usage mistake
(exit 2).

## Talk to a Product's Assistant

```sh
mvdm <product> chat send "Which risks are still open?"
```

`chat` appears in a Product's group when its description holds the conversation operations
(`conversations.create`, `conversations.send-message`, `conversations.list-messages`). `chat send "<text>"` sends
one message to the Product's Assistant, as a person does in the Assistant panel, and waits until its turn ends.
Without `--conversation <id>`, it starts a new conversation. It then prints:

```json
{"conversationId":"…","lastError":null,"queued":false,"messages":[…]}
```

- `messages` holds every message after the one you sent, oldest first, as the API gives it: `seq`, `role`,
  `sender`, the markdown `text`, `attachments`, and `toolActivity` (the tools the Assistant ran, each with its
  outcome).
- `conversationId` is the conversation your message landed in. When the Assistant judges it a new topic, this is
  a new conversation, not the one you named. Continue with that id.
- `lastError` says why the turn stopped early, when it did.
- `queued` is true when your message went into a shared conversation while another member's turn was running.
  The Assistant answers it in the turn after that one, so `messages` may hold only that member's reply. Read the
  answer later with `mvdm <product> conversations list-messages <conversationId> --after <seq> --wait 60`.

**Answer a question or proposal.** The Assistant asks and proposes in the text of its messages. Answer with the
next `chat send`, passing the printed `conversationId`:

```sh
mvdm <product> chat send "Yes, go ahead." --conversation <conversationId>
```

An empty Assistant allowance (402 `allowance-exhausted`) or a conversation that cannot take a message now (409)
prints the ProblemDetails on stderr and exits 1. For everything else about conversations, such as listing them or
stopping a turn, use the generated `mvdm <product> conversations …` commands.

## Raw calls

`mvdm <product> api <method> <path>` makes one call to that Product with your credential. The path is relative to
the Product's host, and a query string rides in it.

```sh
mvdm <product> api GET '/api/v1/risks?$top=5'
mvdm <product> api POST /api/v1/risks --body '{"title":"Flood"}'
mvdm <product> api PATCH /api/v1/risks/r1 --body @risk.json
mvdm <product> api GET /api/v1/reports/q3 --out q3.pdf
```

- `--body <json|@file>` sends a JSON body, inline or read from a file.
- `--out <path>` writes the response body to a file and prints `{"path","bytes","contentType","fileName"}`. A
  response that is not JSON needs `--out`: the CLI never writes raw bytes to stdout.

## Output

- stdout holds the JSON result: pretty-printed in a terminal, compact otherwise. An empty success prints
  `{"ok":true,"status":<code>}`.
- stderr holds errors: the API's ProblemDetails as it came, or `{"error":"<code>","message":"…"}` for a failure
  on your side, such as `not-signed-in`, `network`, `discovery`, `file`, `upload`, or `binary-response`.
- On a rate limit (429) the CLI waits for `Retry-After` and retries, up to three times.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0 | Success. |
| 1 | Error: see stderr. |
| 2 | Usage mistake: fix the command line. |
| 3 | Browser handoff: a person must finish the act in the browser. |

A Browser handoff, from any Product, prints `{"status":"browser_handoff","reason":…,"handoffUrl":…}` on stdout.
Show the link to a person; the CLI never opens a browser for it.

## Environment

| Variable | Meaning |
| -------- | ------- |
| `MVDM_TOKEN` | A Personal token, sent as the bearer to every Product. Wins over the stored sign-in. |
| `MVDM_AUTH_URL` | Auth's address. Defaults to `https://auth.mvdm.io`. |
| `MVDM_<NAME>_URL` | One Product's base URL, such as `MVDM_COMPLIANCE_URL`. Wins over Auth's list. |
| `MVDM_CONFIG_DIR` | The folder for the stored sign-in. |
| `MVDM_CACHE_DIR` | The folder for the cached Product list and API descriptions. |

## Develop

Plain `cargo test` runs the tests against in-process fake Auth and Product hosts. The E2E suite runs the binary
against a Test-bed: a real Compliance built from the `mvdmio-suite` monorepo, with its own throwaway database.
`scripts/test-bed.sh` starts one and prints `{"url": "...", "token": "..."}`; put the two values in
`MVDM_COMPLIANCE_URL` and `MVDM_TOKEN` to run `mvdm compliance …` against it. `cargo test --test e2e -- --ignored`
runs the suite, and uses a Test-bed you already run when `MVDM_E2E_URL` and `MVDM_E2E_TOKEN` are set. The
repository is `mvdmio/mvdm-cli`. See [AGENTS.md](AGENTS.md) for the details.

## Licence

MIT. See [LICENSE](LICENSE).
