# Design

## Context

See proposal.md (Why). Current state, from `crates/gitpm-cli/src/main.rs` at `0b146cf`:

- The CLI parses flat commands from `COMMANDS`, pulls options first (`option`/`flag`), validates option/command pairs, checks the argument count (`expected`), and dispatches. Catalog commands call the library directly: `catalog::browse` (via `gitpm_cli::catalog::info`), `catalog::list`, `catalog::update_sources`, `catalog::migrate`, `catalog::register_package`.
- Every library function the group needs already exists and is already tested; nothing in the library knows the command names.
- Stand-to-gitpm map of the commands this change touches (stand `manager/src/main.rs`):

| Stand `pm` | gitpm 0.5 | After this change |
|---|---|---|
| `info` (prints workspace, two catalog addresses, falls back to `pkg.tilr.dev`) | `info` (needs `--catalog`) | `catalog show` (`info` deprecated alias) |
| `catalog` | `catalog` | `catalog links` (bare `catalog` deprecated alias) |
| `catalog-update [--sources F] [--publish]` | `catalog-update [--sources F]` | `catalog update [--sources F]`; `--publish` not carried (it ran `sync`) |
| `migrate-catalog` | `migrate-catalog` | `catalog migrate` |
| `register SRC --ref R [--build-url U --build-sha256 S]` | same + `--package ID` | `catalog register …` |
| `sync [--dry-run]` | — | excluded (Tilr R2/Cloudflare) |
| `auth check URL` | `auth check URL` (`auth::check`, already in the library) | unchanged |
| `remote REPO URL` | `remote REPO URL` (`scaffold::set_remote`) | unchanged |
| `namespace [NAME]` (`.pm.json`, `PM_NAMESPACE`) | removed in 0.3/0.4 | not added (see Decisions) |
| `create PATH` | `create NS/NAME --into DIR` = `init` | unchanged |

Note: the stand no longer builds against gitpm `dev` (it calls `identity::resolve_namespace`, `identity::write_namespace` and `scaffold::create_namespaced`, removed by `multi-package-repos`); its last green Dagger run (`results/20261016T172214`) predates gitpm 0.4.

## Goals / Non-Goals

**Goals:**
- One canonical spelling per catalog operation: `gitpm catalog SUB`.
- Old spellings keep working with identical stdout, so scripts and the existing CLI tests keep passing during 0.x.

**Non-Goals:**
- New catalog behavior, output formats or flags; JSON output for `catalog show`.
- Library, FFI or catalog format changes.

## Decisions

### D1. Subcommand dispatch inside the existing parser
`catalog` becomes a command whose first positional argument, when present and one of `show|links|update|migrate|register`, selects the subcommand; the effective command name used by validation, help and error messages is the two-word name (`catalog update`). The dispatch maps each subcommand to the code path of its old command, so behavior and output are shared, not duplicated. Alternative: switch the CLI to `clap` with nested subcommands — rejected for this change: it would rewrite all option validation and help text that the `cli` spec pins.

### D2. Old names are deprecated aliases, not removed
`info`, bare `catalog`, `catalog-update`, `migrate-catalog` and `register` run the same code path and print, before anything else, `Warning: \`gitpm OLD\` is deprecated; use \`gitpm catalog NEW\`` on stderr (stdout unchanged; existing tests assert stdout). Removal is a later, **BREAKING** change once the Dagger suite and README use only the new names. Alternative: remove now (0.x allows it) — rejected: no recorded need to break callers, and the stderr warning gives one release of notice.

### D3. Bare `catalog`
Bare `gitpm catalog` keeps its current meaning (JSON links of the catalog root level) as a deprecated alias of `catalog links`. An unknown word after `catalog` fails before opening anything: `unknown catalog command: X; use gitpm catalog --help`. Alternative: bare `catalog` prints the group usage — rejected because it changes the output of an existing command without a BREAKING note.

### D4. Options and validation
Flags keep their rules with the new names: `--sources` only with `catalog update`, `--package` with `install`/`commit`/`catalog register`, `--build-url`/`--build-sha256` (together) and `--ref` with `catalog register` (and `install`/`update`). Messages name the new spelling, for example `--sources requires catalog update`. The deprecated spellings are accepted wherever their new spelling is. The missing-catalog message uses the two-word name: `catalog update needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`. Argument counts: `show`, `links`, `update`, `migrate` take none; `register` takes one (`GIT_SOURCE`).

### D5. Output
Unchanged per operation: `catalog show` prints the overview text (limit line now `… gitpm --catalog <branch location> catalog show`), `catalog links` and `catalog register` print pretty JSON, `catalog update` prints `Reading releases: SOURCE` progress and `Warning: SOURCE: reason` on stderr and `Catalog ready: N release(s) checked from sources.` on stdout, `catalog migrate` prints `Catalog migrated: LOCATION (format v2, one folder per plugin)`. The "one line per affected package" rule of spec `cli` applies to workspace mutations and is not extended to catalog writes here.

### D6. Library API
No additions. The group uses `catalog::browse`, `catalog::list`, `catalog::update_sources`, `catalog::migrate`, `catalog::register_package`; `auth check` keeps `auth::check` with `AuthOptions.custom_ssh` derived by the CLI from `GIT_SSH_COMMAND`/`GIT_SSH` (environment stays in the CLI). All git/curl calls stay in `crates/gitpm/src/exec.rs`.

### D7. Namespace default is not added
The brief asked for a gitpm equivalent of the stand's `namespace` command (`.pm.json`, `PM_NAMESPACE`). The recorded decisions are the opposite: no config file and no namespace command (change `2026-10-16-gitpm-cli`, task 1.3), namespace config removed from the library (`2026-10-17-multi-package-repos`), spec `package-ids` "No default namespace" and spec `cli` "There SHALL be no config file". This change keeps them. Scenarios that relied on a default pass `NAMESPACE/NAME` to `init` and `--ns` to `fork`. If the owner reverses the decision, the smallest consistent design is a CLI-only `GITPM_NAMESPACE` variable used when `init` gets a bare `NAME` or `fork` lacks `--ns` (no file, no command, library signatures unchanged); that would be its own change modifying `package-ids` and `cli`.

### D8. Transactions and crash recovery
No new write paths. `catalog update`, `catalog register` and `catalog migrate` write through the existing catalog writer: exclusive `.<name>.catalog.lock` beside the catalog, unique temp files renamed into place, leftovers of dead writers cleaned (spec `catalog` "Writers are serialized and crash-safe"). The aliases share that code path, so a killed writer behaves identically under either name.

## Risks / Trade-offs

- [Two spellings during the deprecation window] → stdout identical, one stderr warning; tests cover both spellings per operation; removal tracked as a follow-up change.
- [Scripts that parse stderr strictly] → the warning is one `Warning:` line, the existing stderr convention (spec `cli` "Warnings go to stderr").
- [`catalog` positional parsing collides with a future `catalog ID` form] → none exists today; unknown words fail loudly (D3).

## Migration Plan

Users replace `info` → `catalog show`, `catalog` → `catalog links`, `catalog-update` → `catalog update`, `migrate-catalog` → `catalog migrate`, `register` → `catalog register`; until the follow-up removal both work. Rollback: revert the CLI commit; no data or format changes.
