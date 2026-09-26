# Idea — Cover every write operation in the E2E suite

Status: idea

## Motivation

The first E2E suite runs every read operation in Compliance's API description through the CLI's generated commands. But it covers writes with only one hand-picked generated write command. A Compliance change to a write operation's body or parameters can still break the CLI without the E2E suite noticing.

## Goal

The E2E suite runs every write operation (POST, PUT, PATCH, DELETE) in the live API description through the generated commands against a Test-bed. Each write is checked for what it changed, not only for a successful status.

## Decisions (locked)

- This comes after the first E2E suite, which covers every read. It gets a grilling session of its own.

## Out of scope

- Reads. The Read sweep already covers them.

## Open questions

- Where does each write's body come from: generated from the description's schema, or hand-written per operation?
- How does a write prove it worked: by reading the changed record back, or by the status and body alone?
- In what order do writes run, so that a DELETE does not remove what a later write needs? Does each write get a fresh Test-bed database?
- Which writes have side effects that must stay off in a Test-bed, such as sending email, charging through Stripe, or starting an Assistant turn?
