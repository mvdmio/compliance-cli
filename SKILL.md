---
name: mvdm
description: Use the `mvdm` command-line tool to work with the mvdm.io Products' REST APIs on a person's behalf, and to talk to a Product's Assistant. Use it when a person asks you to look something up in one of their mvdm.io Products, change it, upload a file to it, or ask its Assistant. One sign-in serves every Product. Every command prints JSON.
---

# mvdm

`mvdm` calls the REST APIs of the mvdm.io Products for the person you work for. It acts as that person, in one of
their Accounts at a time. One sign-in works in every Product. Every command prints JSON on stdout, and the exit
code says what happened.

Use it when the person asks about or wants to change anything they keep in a Product. Prefer a command over raw
HTTP: the CLI signs in, builds the URL and body, splits large files, and retries on a rate limit.

Install, or upgrade, with:

```sh
curl -LsSf https://auth.mvdm.io/cli/install.sh | sh
```

On Windows: `powershell -ExecutionPolicy ByPass -c "irm https://auth.mvdm.io/cli/install.ps1 | iex"`.

## Find the Products

- `mvdm --help` lists the Products you can use, each with its display name, and the top-level commands.
- `mvdm <product> --help` lists that Product's commands and ends with `Docs: <address>`, the Product's guides.
  Read them for what the Product does and how its data fits together.
- `https://<the Product's host>/llms.txt` lists what that Product holds, in a form made for Agents.

A new Product appears in `mvdm --help` by itself; the CLI needs no update for it. A Product name the CLI does not
know is a usage mistake (exit 2).

## Sign in

Run `mvdm status` first. It prints `"signedIn": true` or `false`, the credential in use, and the Account commands
act in; `user` is `null` with a Personal token. Without a credential it exits 0. With one that no longer works it
exits 1.

- `mvdm login` opens the person's browser at Auth. The person signs in and allows the CLI. You wait; the command
  ends when they have. This one sign-in works for every Product.
- `mvdm login --device` prints a link with the code filled in, and the code, on stderr. Show both to the person;
  they finish on any device. `login` falls back to this by itself when it cannot open a browser, such as over SSH.
- `MVDM_TOKEN` holds a Personal token for unattended runs, such as CI. It wins over the stored sign-in and serves
  every Product. Only the person can create one, on the API & Agent Access page of their account. Never ask them
  to paste one into the chat; ask them to set the variable.
- `MVDM_AUTH_URL` points the CLI at another Auth. Leave it unset for `https://auth.mvdm.io`.
- `MVDM_<NAME>_URL` points one Product at another host, such as a local build, and wins over the Product list.
  `<NAME>` is the Product's command name in capitals. Set it only when the person asks.
- `mvdm logout` ends this CLI's stored sign-in at Auth and forgets it; `MVDM_TOKEN` stays. Do not run it unless the
  person asks.

A command that fails with `not-signed-in` needs `mvdm login`.

## Accounts

One credential acts in one Account at a time, in every Product.

- `mvdm accounts list` lists the person's Accounts. The one commands act in has `"current": true`.
- `mvdm accounts switch <id>` moves the credential into another Account, for every later command in every
  Product. Switch only when the person asks for another Account, and say which one you switched to.

## Commands

Every REST operation of a Product is a command: `mvdm <product> <group> <action>`, both in kebab-case, from its
`operationId` `<group>.<action>`. The CLI reads the Product's live OpenAPI document, so a new operation works at
once. The group comes from the `operationId`, not the tag.

- `mvdm <product> --help` lists the groups. `mvdm <product> <group> --help` lists its actions.
  `mvdm <product> <group> <action> --help` shows what the operation does and every argument with its type. Read it
  before your first call of an action.
- Path parameters are positional arguments, in path order.
- Query parameters are options, such as the OData options `--filter`, `--select`, `--orderby`, `--expand`,
  `--top`, and `--skip` where the operation takes them: `mvdm <product> <group> list --filter "status eq 'Open'"
  --top 5`.
- Top-level body fields are options, kebab-cased: `collectedAt` is `--collected-at`. A list option repeats.
- `--body '<json>'` or `--body @file.json` sends a whole body, for nested shapes. Field options win over the same
  keys in it.
- `mvdm <product> api <method> <path> [--body …] [--out …]` makes one raw call to that Product, for anything the
  commands do not shape well. The path is relative to the Product's host.

A group or action the CLI does not know makes it fetch the Product's description again. A command that is still
unknown is a usage mistake.

## Output and exit codes

| Code | Meaning | What to do |
| ---- | ------- | ---------- |
| 0 | Success. stdout holds the JSON result. | Read the result. |
| 1 | Error. stderr holds the API's ProblemDetails, or `{"error":"<code>","message":…}`. | Read `title`, `detail`, or `message`, fix the cause or tell the person. |
| 2 | Usage mistake. | Fix your command line; check `--help`. |
| 3 | Browser handoff. | See below. |

On a rate limit (429) the CLI waits and retries by itself, up to three times. Do not retry an exit 1 in a loop.

## Browser handoff

Some acts need the person in the browser, such as paying or confirming something only they may confirm. Any
Product can ask for it. Then the command exits 3 and prints:

```json
{"status":"browser_handoff","reason":"…","handoffUrl":"https://…"}
```

Show the `handoffUrl` to the person and tell them the `reason`. Never open the link yourself, and never try to
finish the act another way. Once the person says they are done, go on.

## Files

Where a Product offers them:

- **Upload.** An action that takes an upload id also takes `--file <path>`. The CLI sends the file and then makes
  the call with the new id. Do not pass the id option too. Where the action takes a list of ids, repeat `--file`.
  Large files go in parts and resume by themselves. `--help` of the action shows whether it has `--file`.

  ```sh
  mvdm <product> <group> create --file scan.pdf --title "Scan"
  ```

- **Download.** An action that answers a file needs `--out <path>`. The CLI writes the file there and prints
  `{"path","bytes","contentType","fileName"}`. It never writes file bytes to stdout.

  ```sh
  mvdm <product> <group> download 8d1e --out scan.pdf
  ```

## Talk to a Product's Assistant

A Product that offers its Assistant over the API has `mvdm <product> chat send` in `mvdm <product> --help`. The
Assistant knows the Account and can act in it. Ask it when the person wants its judgement, such as an assessment or
a proposal, rather than one record.

```sh
mvdm <product> chat send "Which items are still open?"
```

It waits until the Assistant's turn ends and prints
`{"conversationId","lastError","queued","messages":[…]}`. Each message has `role`, `sender`, the
markdown `text`, and `toolActivity`.

- **Continue** with `--conversation <conversationId>`, always the id from the last output: it changes when the
  Assistant moves the message to a new topic.
- **Questions and proposals** from the Assistant are in the text of its last message. Put them to the person, or
  answer them yourself when the person already told you what they want, with the next `chat send` on the same
  conversation: `mvdm <product> chat send "Yes, go ahead." --conversation <conversationId>`.
- **`queued: true`** means another member's turn ran first, and `messages` may not hold your answer yet. Read it
  later with `mvdm <product> conversations list-messages <conversationId> --after <seq> --wait 60`, where
  `<seq>` is the last message you saw.
- **`lastError`** says why a turn stopped early. An exit 1 with `allowance-exhausted` means the Account's
  Assistant allowance is used up; tell the person.

## Privacy

The CLI talks only to Auth and to the Products it is pointed at. It sends no usage data.
