# AGENTS.md

Guidance for agents working in this repository.

## What this repo is

A Rust workspace that talks to **Odoo 19** over its **JSON-2 API**: a library
(`crates/hodoo`) plus a CLI (`crates/hodoo-cli`, binary `hodoo`). There is no
application server here: the repo *is* the client. `README.md` is the
user-facing document; `docs/superpowers/specs/` holds the design records behind
the CLI's shape.

The Odoo instance this was written against runs on a 512 MB Debian 13 VM (Odoo,
PostgreSQL 17, nginx). That deployment lives in a **separate repository**
(`odoo-512mb`); nothing here deploys or manages it. A Topcoat web app
(`hodoo-web`) may join the workspace as a third member.

## Toolchain

Rust 1.85+ is required (edition 2024). It is installed under `~/.cargo/bin`,
which is **not on `PATH`** in a plain shell: run `~/.cargo/bin/cargo` or
`export PATH="$HOME/.cargo/bin:$PATH"` first. The `just` recipes set this
themselves.

| Command | Does |
|---|---|
| `cargo test` | Unit tests, stub-HTTP tests (`crates/hodoo/tests/http.rs`, wiremock), doctests |
| `cargo clippy --all-targets -- -D warnings` | The lint gate; `unwrap_used`/`expect_used` are warnings, so no unwrapping outside tests |
| `just check` | The whole gate: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |
| `just run -- <args>` | Runs the client (`just run -- task ls --project acme`); builds it first, quietly |
| `just test` | The offline suite alone |
| `just live-test` | The two server suites (`HODOO_LIVE=1`): `tests/live.rs` and `tests/drift.rs` |
| `just prod-build` | Release build against the committed lock file |
| `just install [prefix]` | Install the release binary onto `PATH` (needs root) |
| `just doctor` | Is this checkout ready? Toolchain, binary, credentials, server |
| `just clean` | Build output and the scenario scratch files |
| `just scenario <up\|down\|show>` / `just icare-dd <up\|down\|show>` | The two worked datasets |
| `cargo fmt --check` | Formatting (run `cargo fmt` to fix) |

`just live-test` runs `tests/live.rs` (creates and deletes real records) and
`tests/drift.rs` (field names vs Odoo's `/doc-bearer/<model>.json`; needs a
Settings-level key, otherwise it prints a skip).

## CLI conventions (the human-first rework)

The CLI is for people: a table by default, colour only on a terminal, deadlines
as `in 3d`, and an id **or a name** wherever a record is referenced
(`--project acme`, `--stage Review`, `--tag urgent`; `--tag` creates a missing
tag). Scripts opt into JSON with `-o json` / `--json` / `HODOO_OUTPUT=json` -
worth setting once at the top of a script.
`docs/superpowers/specs/2026-09-28-hodoo-cli-ux-design.md` records why, with
the clig.dev rules behind each choice; `README.md` is the user-facing version.

**"Stage" is two different models, and every stage command is under
`hodoo project`.** A task's stage is a `project.task.type`, a kanban column:
`project task-stages ls <project>`, `project task-stages create|update|rm`, and
`task --stage` to move a task into one. A *project's* stage is a
`project.project.stage`, one of a handful the server shares:
`project stages ls|create|update|rm`, and `project update --stage` to move a
project into one. `project attach`/`detach` take `--task-stage`. There is no
top-level stage command: a stage is a project's business either way. Both `rm`s
refuse while a record is still in the stage and name it, because Odoo answers
with a bare `ValidationError` naming only the model.

Things that will bite an agent writing commands into a script:

- **`rm` refuses without `-f` when stdin is not a terminal** (and without a `y`
  on one). `--no-input` makes it fail with the reason instead of hanging.
- **`-n`/`--dry-run` sends nothing** - mutations print what they *would* send.
  Use it to check a command's shape without touching data.
- **A change reports itself on stdout in JSON mode**: `{"id":31}` for a create,
  `{"id":31,"ok":true}` for an update, `{"deleted":31,"ok":true}` for a delete.
  In table mode the confirmation goes to stderr instead, so stdout stays
  parseable either way.
- **Long text goes through stdin**: `--body -` / `--description -` read it, so
  a report or an HTML description needs no shell quoting.
- **Dates without an offset are local time**, and `+3x` or `soon` is an error, not
  a guess. Pass an RFC 3339 stamp with an offset when a script must be exact.
- **A filter tag must exist**: `task ls --tag typo` fails with exit 2 and creates
  nothing; only a real create/update makes a missing tag (`-n` shows it as `(new)`).
- **`call` takes its body as `--body`**, not `--json` (`--json` is the output
  flag now).
- **A state write can lose to Odoo's compute**: a task with open dependencies
  reads `waiting` whatever `--state` you write, and the CLI now says so on
  stderr when it happens. Close the blockers first, or expect `waiting`.
- **Deletes name what cascades** (a project takes its tasks and milestones, a
  task takes its subtasks) in the prompt and the dry run, so `-f` is never a
  blind guess.
- Names that match several records are an error listing them, never a guess.

Credentials resolve flag, then process environment (`ODOO_URL`, `ODOO_API_KEY`,
`ODOO_DB`), then a `.env` at or above the working directory. `.env` is
gitignored and read into a map rather than exported: `std::env::set_var` is
unsafe in edition 2024 and the crate forbids unsafe code. The same layering is
available to library callers as `Config::from_env()` plus `hodoo::dotenv`. JSON
output uses **Odoo's** field names (`date_deadline`, `user_ids`,
`privacy_visibility`, `type_ids`) in both directions, which is what
`#[serde(rename)]` on the read structs is for.

`hodoo version --url <server>` needs no API key and is the cheapest way to check
a server is reachable; `hodoo whoami` proves url, certificate and key together
(it answers `res.users/context_get`, whose `uid` is the key's user). Failures
exit `1` (Odoo/transport) or `2` (usage/config) with a JSON object on stderr; a
field Odoo does not have comes back as
`{"kind":"odoo","status":500,"message":"Invalid field ..."}`, so the escape
hatch tells you when a model changed.

## Building a dataset over the CLI: read the SOP first

`scenarios/startup-founder.sh {up|down|show}` (or `just scenario up`) builds a
4-project dataset over the CLI, asserting 46 properties as it goes: two client
websites, an internal product and personal life, with task stages, tags,
milestones, subtasks, dependency chains, chatter and every state. Everything it
creates carries a `(scenario)` marker, and `down` deletes by that marker only,
so it can never touch Odoo's own records. `up` rebuilds from scratch (it runs
`down` first) and `down` is safe to run twice.

The second worked example is `scenarios/icare-dd.sh {up|down|show}` (`just
icare-dd up`): a manager-level due diligence of a fund manager as one project -
15 workstreams x 5 phases, 4 phase gates, 7 milestones, 79 tasks, marker
`(icare-dd)`, assertions as it builds. Every task description carries
`Workstream`/`Owner`/`Standard`/`Evidence`/`Acceptance` lines, urgency maps to a
`red`/`amber`/`green` tag, and `up` fails if a red item has no `Remediation`
line. The strategy, standards matrix, RACI and rating rules behind it are in
`docs/superpowers/specs/2026-09-28-icare-manager-dd-design.md`.

**Before writing a third one, read `scenarios/SOP.md`.** It exists because the
iCare build hit seven failures from one root cause: an assumption about Odoo, or
about the script's own data, typed as a constant. The rules it distils, in
short:

- **Derive every count from the data table**, never type a total into an
  assertion (`ITEM_COUNT`, `WS_COUNT`, `TASKS_TOTAL` in `icare-dd.sh`). Odoo also
  keeps **one** tag model for projects and tasks, so project tags count towards a
  tag total.
- **Guard a rebuild on a residue sum of everything the script creates**
  (projects, task stages, tags, milestones). An interrupted teardown leaves
  orphan stages that belong to no project, and a projects-only guard walks past
  them into duplicates.
- **Re-query child ids after a cascade.** Deleting a project takes its tasks,
  milestones and chatter with it, so a milestone id list gathered before the
  delete 404s.
- **`state` is computed from open blockers**, so a write loses to the compute:
  set a state only where a task's blockers are closed, and pin the surprising
  direction as an assertion rather than fighting it (Odoo does not compute it
  during creation either).
- **Never parse a table with `awk`.** In JSON mode there is no table, `--limit 0`
  means unlimited, and `board`'s JSON is one object holding every open task while
  its table groups by column.
- **Assert invariants, not just counts** - "no task is missing its four lines",
  "no red without a remediation" - and end teardown by asserting zero of each
  kind it created.

## JSON-2 facts to know before touching `crates/`

- The whole API is `POST /json/2/<model>/<method>`, `Authorization: Bearer <api
  key>` (`auth='bearer'`), named arguments at the top level of the body plus
  optional `ids` and `context`. `X-Odoo-Database` is only needed with several
  databases behind one domain, so it is opt-in.
- `create` takes `vals_list` and **answers a list of ids** (`[7]`), because
  JSON-2 reduces a returned recordset to its ids. Errors are `{name, message,
  arguments, context, debug}` with a Python exception name and a real HTTP
  status.
- Keys are per user and last at most three months. There is no password login
  over JSON-2, so nothing here can fall back to one.
- Odoo's own method discovery lives at `/doc` (browser) and
  `/doc-bearer/*.json` (bearer); the latter needs `base.group_system`.
- Odoo returns `false` (not `null`) for an unset `Char`/`Text`/`Html` field and
  answers many2one as `[id, "Name"]`; `crates/hodoo/src/de.rs` is the one place
  that tolerates both. Reads always send an explicit `fields` list, or Odoo
  returns every computed column.
- Only reads are retried (once, on a connection failure or 502/503/504), because
  the Odoo this targets is OOM-killable and JSON-2 has no idempotency key for
  writes.

## Style conventions

- **ASCII `--` instead of em dashes** in code, comments and Markdown prose.
  Keep it.
- Comments explain *why* a choice exists (the specific failure it prevents), not
  *what* the line does - that is the most consistent thing about this codebase.
- The crates hold themselves to `#![forbid(unsafe_code)]`, `missing_docs`,
  `clippy::unwrap_used`/`expect_used`, and `cargo clippy --all-targets -- -D
  warnings`. Keep the tree warning-free.
- Deliberate simplifications with a known ceiling carry a `ponytail:` comment
  naming the ceiling and the upgrade path.
- **Commit messages are conventional commits** (`feat:`, `fix:`, `docs:`,
  `refactor:`, `perf:`, `test:`, `chore:`), imperative lowercase subject under
  72 characters, then a body explaining the failure that motivated the change
  and what now prevents it.
