# Design

## Context

See proposal.md (Why). State this change starts from: `crates/gitpm-cli/src/main.rs` at `0b146cf` as changed by the 2026-10-18 chain (`no-backward-compat` … `host-compatibility`, plus the per-task changes split from the former `robustness-fixes`):

- The CLI parses flat commands from `COMMANDS`, pulls options first (`option`/`flag`), validates option/command pairs, checks the argument count (`expected`), and dispatches. Catalog commands call the library directly: `catalog::browse` (via `gitpm_cli::catalog::info`), `catalog::list`, `catalog::update_sources`, `catalog::register_package`.
- Every library function the group needs already exists and is already tested. After `thin-cli-boundary` library messages name no command; the CLI adds hints per error kind, and this change only updates the hint texts to the new spellings (D6).
- Stand-to-gitpm map of the commands this change touches (stand `manager/src/main.rs`):

| Stand `pm` | gitpm 0.5 | After this change |
|---|---|---|
| `info` (prints workspace, two catalog addresses, falls back to `pkg.tilr.dev`) | `info` (needs `--catalog`) | `catalog show` (`info` removed) |
| `catalog` | `catalog` | `catalog links` (bare `catalog` fails naming the subcommands) |
| `catalog-update [--sources F] [--publish]` | `catalog-update [--sources F]` | `catalog update [--sources F]`; `--publish` not carried (it ran `sync`) |
| `migrate-catalog` | `migrate-catalog` | removed by `no-backward-compat` (TILR-38) |
| `register SRC --ref R [--build-url U --build-sha256 S]` | same + `--package ID` | `catalog register …` |
| `sync [--dry-run]` | — | excluded (Tilr R2/Cloudflare) |
| `auth check URL` | `auth check URL` (`auth::check`, already in the library) | unchanged |
| `remote REPO URL` | `remote REPO URL` (`scaffold::set_remote`) | unchanged |
| `namespace [NAME]` (`.pm.json`, `PM_NAMESPACE`) | removed in 0.3/0.4 | not added (see Decisions) |
| `create PATH` | `create NS/NAME --into DIR` = `init` | removed by `no-backward-compat`; `init` only |

Note: the stand no longer builds against gitpm `dev` (it calls `identity::resolve_namespace`, `identity::write_namespace` and `scaffold::create_namespaced`, removed by `multi-package-repos`); its last green Dagger run (`results/20261016T172214`) predates gitpm 0.4.

## Goals / Non-Goals

**Goals:**
- One canonical spelling per catalog operation: `gitpm catalog SUB`.
- The rename is marked BREAKING and listed in README "Changed in 0.6" (owner decision of 2026-10-18 on TILR-8, variant (c); TILR-38).

**Non-Goals:**
- New catalog behavior, output formats or flags; JSON output for `catalog show`.
- Library, FFI or catalog format changes.

## Decisions

### D1. Subcommand dispatch inside the existing parser
`catalog` becomes a command whose first positional argument, when present and one of `show|links|update|register`, selects the subcommand; the effective command name used by validation, help and error messages is the two-word name (`catalog update`). The dispatch maps each subcommand to the code path of its old command, so behavior and output are shared, not duplicated. Alternative: switch the CLI to `clap` with nested subcommands — rejected for this change: it would rewrite all option validation and help text that the `cli` spec pins.

### D2. Old names are removed
`info`, `catalog-update` and `register` are removed and fail as unknown commands (`unknown command: info; use --help`), as the owner decided (variant (c) of TILR-8).

### D3. Bare `catalog`
Bare `gitpm catalog` (no subcommand) fails before opening anything with `catalog needs a subcommand: show, links, update or register; use gitpm catalog --help`. An unknown word after `catalog` fails with `unknown catalog command: X; use gitpm catalog --help`. Alternative: bare `catalog` prints the group usage and exits 0 — rejected: a missing subcommand is a usage error like a missing argument elsewhere.

### D4. Options and validation
Flags keep their rules with the new names: `--sources` only with `catalog update`, `--package` with `install`/`download`/`commit`/`catalog register`, `--build-url`/`--build-sha256` (together) and `--ref` with `catalog register` (and `install`/`download`/`update`). These are syntax checks; domain combinations stay in the library (`thin-cli-boundary`). Messages name the new spelling, for example `--sources requires catalog update`. The missing-catalog message uses the two-word name: `catalog update needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG`. Argument counts: `show`, `links`, `update` take none; `register` takes one (`GIT_SOURCE`).

### D5. Output
Unchanged per operation: `catalog show` prints the overview text (limit line now `… gitpm --catalog <branch location> catalog show`), `catalog links` and `catalog register` print pretty JSON, `catalog update` prints `Reading releases: SOURCE` progress and `Warning: SOURCE: reason` on stderr and `Catalog ready: N release(s) checked from sources.` on stdout, The "one line per affected package" rule of spec `cli` applies to workspace mutations and is not extended to catalog writes here.

### D6. Library API
No library change. The group uses `catalog::browse`, `catalog::list`, `catalog::update_sources` and `catalog::register_package`. `update_sources` on an HTTP(S) location already fails with `invalid_argument` and a neutral text (`thin-cli-boundary`); the CLI hint for it now names `gitpm catalog update`. `auth check` is untouched (`Options.custom_ssh` from `one-ssh-policy-custom-ssh`). All git/curl calls stay in `crates/gitpm/src/exec.rs`.

### D7. Namespace default is not added
The brief asked for a gitpm equivalent of the stand's `namespace` command (`.pm.json`, `PM_NAMESPACE`). The recorded decisions are the opposite: no config file and no namespace command (change `2026-10-16-gitpm-cli`, task 1.3), namespace config removed from the library (`2026-10-17-multi-package-repos`), spec `package-ids` "No default namespace" and spec `cli` "There SHALL be no config file". This change keeps them. Scenarios that relied on a default pass `NAMESPACE/NAME` to `init` and `--ns` to `fork`. If the owner reverses the decision, the smallest consistent design is a CLI-only `GITPM_NAMESPACE` variable used when `init` gets a bare `NAME` or `fork` lacks `--ns` (no file, no command, library signatures unchanged); that would be its own change modifying `package-ids` and `cli`.

### D8. Transactions and crash recovery
No new write paths. `catalog update` and `catalog register` write through the existing catalog writer: exclusive `.<name>.catalog.lock` beside the catalog, unique temp files renamed into place, leftovers of dead writers cleaned (spec `catalog` "Writers are serialized and crash-safe"). The new spellings call the same writer, so a killed writer behaves as before.

## Risks / Trade-offs

- [Scripts using the old names break] → BREAKING note and rename table in README "Changed in 0.6"; the failure is an immediate `unknown command`, never a silent behavior change.
- [`catalog` positional parsing collides with a future `catalog ID` form] → none exists today; unknown words fail loudly (D3).

## Migration Plan

Users replace `info` → `catalog show`, `catalog` → `catalog links`, `catalog-update` → `catalog update`, `register` → `catalog register`. The Dagger suite switches through `gitpmtest.COMMANDS` in the same change. Rollback: revert the CLI commit; no data or format changes.
