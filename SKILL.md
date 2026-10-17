---
name: compliance
description: Use the `compliance` command-line tool to read and change a Compliance Account (risks, controls, evidence, frameworks, imports, and every other register) through its REST API, and to talk to its Assistant. Use it when a person asks you to look something up in Compliance, change it, upload a file to it, or ask its Assistant. Every command prints JSON.
---

# compliance

`compliance` calls the Compliance REST API at `https://compliance.mvdm.io` for the person you work for. It acts
as that person, in one of their Accounts at a time. Every command prints JSON on stdout, and the exit code says
what happened.

Use it when the person asks about or wants to change anything they keep in Compliance. Prefer a command over raw
HTTP: the CLI signs in, builds the URL and body, splits large files, and retries on a rate limit.

Install, or upgrade, with:

```sh
curl -LsSf https://compliance.mvdm.io/cli/install.sh | sh
```

On Windows: `powershell -ExecutionPolicy ByPass -c "irm https://compliance.mvdm.io/cli/install.ps1 | iex"`.

## Sign in

Run `compliance status` first. It prints `"signedIn": true` or `false`, the credential in use, and the Account
commands act in; `user` is `null` with a Personal token. Without a credential it exits 0. With one that no longer
works it exits 1.

- `compliance login` opens the person's browser at Auth. The person signs in and allows the CLI. You wait; the
  command ends when they have.
- `compliance login --device` prints a link with the code filled in, and the code, on stderr. Show both to the
  person; they finish on any device. `login` falls back to this by itself when it cannot open a browser, such
  as over SSH.
- `COMPLIANCE_TOKEN` holds a Personal token for unattended runs, such as CI. It wins over the stored sign-in.
  Only the person can create one, on the API & Agent Access page of their account. Never ask them to paste one
  into the chat; ask them to set the variable.
- `COMPLIANCE_URL` points the CLI at another Compliance host. Leave it unset for `https://compliance.mvdm.io`.
- `compliance logout` ends this CLI's stored sign-in at Auth and forgets it; `COMPLIANCE_TOKEN` stays. Do not run
  it unless the person asks.

A command that fails with `not-signed-in` needs `compliance login`.

## Accounts

- `compliance accounts list` lists the person's Accounts. The one commands act in has `"current": true`.
- `compliance accounts switch <id>` moves the credential into another Account, for every later command. Switch
  only when the person asks for another Account, and say which one you switched to.

## Commands

Every REST operation is a command: `compliance <group> <action>`, both in kebab-case, from its `operationId`
`<group>.<action>`. The operation `risks.accept` is `compliance risks accept <id>`. The group comes from the
`operationId`, not the tag: `frameworks.start-gap-analysis` is `compliance frameworks start-gap-analysis`.

- `compliance --help` lists the groups. `compliance <group> --help` lists its actions.
  `compliance <group> <action> --help` shows what the operation does and every argument with its type. Read it
  before your first call of an action.
- Path parameters are positional arguments, in path order.
- Query parameters are options. List actions take the OData options `--filter`, `--select`, `--orderby`,
  `--expand`, `--top`, and `--skip`, as in `compliance risks list --filter "status eq 'Open'" --top 5`.
- Top-level body fields are options, kebab-cased: `collectedAt` is `--collected-at`. A list option repeats.
- `--body '<json>'` or `--body @file.json` sends a whole body, for nested shapes. Field options win over the same
  keys in it.
- `compliance api <method> <path> [--body …] [--out …]` makes one raw call, for anything the commands do not
  shape well: `compliance api GET '/api/v1/risks?$top=5'`.

A group or action the CLI does not know makes it fetch the API's description again, so a new operation works at
once. A command that is still unknown is a usage mistake.

## Output and exit codes

| Code | Meaning | What to do |
| ---- | ------- | ---------- |
| 0 | Success. stdout holds the JSON result. | Read the result. |
| 1 | Error. stderr holds the API's ProblemDetails, or `{"error":"<code>","message":…}`. | Read `title`, `detail`, or `message`, fix the cause or tell the person. |
| 2 | Usage mistake. | Fix your command line; check `--help`. |
| 3 | Browser handoff. | See below. |

On a rate limit (429) the CLI waits and retries by itself, up to three times. Do not retry an exit 1 in a loop.

## Browser handoff

Some acts need the person in the browser, such as paying or confirming something only they may confirm. Then the
command exits 3 and prints:

```json
{"status":"browser_handoff","reason":"…","handoffUrl":"https://…"}
```

Show the `handoffUrl` to the person and tell them the `reason`. Never open the link yourself, and never try to
finish the act another way. Once the person says they are done, go on.

## Files

- **Upload.** An action that takes an upload id, such as `evidence create`, also takes `--file <path>`. The CLI
  sends the file and then makes the call with the new id. Do not pass the id option too. Where the action takes
  a list of ids, repeat `--file`. Large files, such as Import ZIPs, go in parts and resume by themselves.

  ```sh
  compliance evidence create --file scan.pdf --title "Scan"
  ```

- **Download.** An action that answers a file needs `--out <path>`. The CLI writes the file there and prints
  `{"path","bytes","contentType","fileName"}`. It never writes file bytes to stdout.

  ```sh
  compliance evidence download 8d1e --out scan.pdf
  ```

## Talk to the Assistant

Compliance has its own Assistant, which knows the Account and can act in it. Ask it when the person wants its
judgement, such as an assessment or a proposal, rather than one record.

```sh
compliance chat send "Which risks are still open?"
```

It waits until the Assistant's turn ends and prints
`{"conversationId","lastError","queued","messages":[…]}`. Each message has `role`, `sender`, the
markdown `text`, and `toolActivity`.

- **Continue** with `--conversation <conversationId>`, always the id from the last output: it changes when the
  Assistant moves the message to a new topic.
- **Questions and proposals** from the Assistant are in the text of its last message. Put them to the person, or
  answer them yourself when the person already told you what they want, with the next `chat send` on the same
  conversation: `compliance chat send "Yes, go ahead." --conversation <conversationId>`.
- **`queued: true`** means another member's turn ran first, and `messages` may not hold your answer yet. Read it
  later with `compliance conversations list-messages <conversationId> --after <seq> --wait 60`, where `<seq>` is
  the last message you saw.
- **`lastError`** says why a turn stopped early. An exit 1 with `allowance-exhausted` means the Account's
  Assistant allowance is used up; tell the person.

## Privacy

The CLI talks only to the Compliance host and its Auth. It sends no usage data.
