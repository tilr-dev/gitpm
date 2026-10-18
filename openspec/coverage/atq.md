# ATQ checks mapped to specs

The ATQ integration queue (project `tilr-package-manager-integration`, run `20261015-222157`, 2026-10-15/16) tested the stand CLI `pm` with 42 → 56 → 68 task prompts. They are not ported as code; this table maps every check of every task to the spec scenario that now owns the behavior, so coverage can be compared later (TILR-5). Written by hand; `coverage.md` next to it is generated.

## Sources

- Task texts: `~/projects/millai/queue-todo/projects/tilr-package-manager-integration/template/001.md`…`068.md` and its `prompt.md` (the ATQ project folder; same text as the MCP service).
- Run verdicts: `…/tilr-package-manager-integration/runs/20261015-222157/NNN.result.md` / `NNN.failed.md`; artifacts `~/projects/tilr/temp-plugin-manager/atq-artifacts/20261015-222157/`.
- Port decisions: `openspec/changes/archive/2026-10-18-dagger-integration-tests/design.md` (D5, D6) and `itests/README.md` "Not ported"; cases `itests/scenarios/uc_NNN.py` keep the ATQ numbers.
- Checked against code at `f3fa146` (`dev`) and `gitpm 0.5.0-dev+f3fa146`.

Date: 2026-10-18. Tasks: 68. Checks: 314.

| Status | Checks | Meaning |
|---|---|---|
| covered | 277 | a main-spec requirement/scenario states it (a Dagger case or a named cargo test verifies it) |
| missing → added | 10 | behavior existed in code without a scenario; scenario added in this change |
| obsolete | 22 | removed or changed by design (decision cited) |
| design-pending | 5 | stated only by an active change (slug cited) |

Spec references read `capability › Requirement › Scenario`; several scenarios are separated by `/`, several requirements by `;`. "Note" names an active change that will alter a covered check later.

## Late red findings

- **UC-016, tag with a foreign package ID.** `releases::discover_all` (`crates/gitpm/src/releases.rs:133-141`): an `ID@V` tag whose manifest has another ID is skipped with the reason `tag ID@V disagrees with manifest ID OTHER`; a `vV` tag with another ID is skipped silently on purpose (forks keep upstream tags). If the skipped tag is the newest, `ensure_latest_is_valid` (`releases.rs:186-200`) refuses: `version_conflict` "latest release tag is broken: …" (checked by hand and by `uc_016:id_change_in_release_tag_is_rejected`). `install` prints the skip (`crates/gitpm-cli/src/main.rs:1078-1082`). A single `update` uses `releases::latest` (`releases.rs:209-216`, called at `crates/gitpm/src/workspace.rs:986`), which throws the skipped list away: an older foreign-ID or mismatched tag is skipped **without a warning**, against package-ids › Broken historical tags are skipped › One bad old tag (Defect D1).
- **restore loses the catalog origin.** Confirmed: `Manager::restore` of an own clone builds the record without `catalog` (`crates/gitpm/src/workspace.rs:1207-1215`); by hand `install-catalog` → `forget` → `restore` leaves `plugins.dan/hello.catalog` = null. The main spec says so on purpose (install-uninstall › Restore preconditions › Restored record: "no `catalog`"); the fix is decided in `single-ref-mode` (TILR-59): design-pending (row 068.6).

## Defects found

- **D1** `update ID` prints no `Warning: skipped release tag …` for broken historical tags (spec package-ids › Broken historical tags are skipped › One bad old tag: "install and update warn"). Cause: `workspace.rs:986` calls `releases::latest`, which drops `Discovery.skipped` (`releases.rs:209-216`); `RevisionChange` carries no skipped tags. Repro (no catalog): `gitpm init dan/hello --into hello && gitpm install ./hello && git -C hello tag dan/hello@0.1.5 dan/hello@0.1.0`, then set `version` 0.2.0 in `hello/plugin.json`, commit, tag `dan/hello@0.2.0`; `gitpm update dan/hello` prints only `dan/hello updated 0.1.0 → 0.2.0`, while `install ./hello` into a new root prints `Warning: skipped release tag dan/hello@0.1.5: …`. `thin-cli-boundary` (cli › Warnings go to stderr) would print every warning of a result, but no change adds skipped tags to the update result.

## Scenarios added

- authoring › remote sets origin without network › Other remotes are kept (007.2)
- git-safety › Credentials in URLs are the user's responsibility › Query or fragment in an HTTP URL (009.2)
- catalog › Catalog generated from Git release tags › Duplicate source lines (031.3)
- package-ids › Release tag names › Tags that are not release versions (033.5)
- catalog › Bounded overview › Overview lines (034.1)
- catalog › v1 catalogs stay readable › Migrate a v2 catalog (037.3, 062.7)
- catalog › v1 catalogs stay readable › Register into a v1 catalog (037.4)
- cli › Option validation › Flag position (063.4)
- catalog › check compares installed with the catalog › Exit status (063.8)

## Mapping

| ATQ | Check | Spec › Requirement › Scenario | Status | Note |
|---|---|---|---|---|
| 001.1 | help, -h, help describe the commands | cli › Help; cli › Command set › Every command documented | covered |  |
| 001.2 | list on an empty explicit --root is valid JSON | cli › JSON output for state and catalog › list | covered |  |
| 001.3 | missing arguments, unknown command, missing option value fail | cli › Option validation; cli › Command set › Removed command | covered |  |
| 001.4 | unsupported channel, --channel with --ref | cli › Option validation › Channel and ref | covered | `single-ref-mode` (D6, TILR-40) replaces `--channel` |
| 001.5 | --publish outside catalog-update, --dry-run outside sync | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests`; replaced by `--dry-run` outside `update all` (cli › Option validation › Misplaced flag) |
| 001.6 | build flags outside register | cli › Option validation › Misplaced flag | covered |  |
| 001.7 | invalid invocations create nothing | install-uninstall › Refused input creates nothing | covered |  |
| 002.1 | --root/--catalog/--sources beat environment | cli › Settings come from flags, environment, defaults | covered |  |
| 002.2 | stand wrapper keeps cwd, relative init, .env sourced twice | — | obsolete | stand wrapper `./pm` and `.env` are not part of gitpm (`2026-10-16-gitpm-cli`; itests README "Not ported") |
| 002.3 | direct binary defaults in an isolated cwd; old PM_* ignored | cli › Settings come from flags, environment, defaults › Legacy variable | covered |  |
| 003.1 | namespace precedence explicit > env > config > local | package-ids › No default namespace | obsolete | no namespace config or command (`2026-10-16-gitpm-cli` task 1.3) |
| 003.2 | empty, slash, dot-dot, non-ASCII, >80-char namespaces refused, no writes | package-ids › Package ID syntax › Unsafe components are refused / Empty namespace | covered |  |
| 003.3 | underscore, hyphen, digits accepted | package-ids › Package ID syntax › Valid scoped ID | covered |  |
| 003.4 | changing the default namespace renames nothing | — | obsolete | no default namespace; IDs stay on release/update (`uc_003:release_and_update_keep_the_id`) |
| 004.1 | scaffold: plugin.json, dist/index.js, README, .gitignore, one commit, ID@0.1.0 tag | authoring › init creates a package with its first release › Scaffold files and commit | covered |  |
| 004.2 | nested and absolute paths with spaces | authoring › init creates a package with its first release › New package repository | covered |  |
| 004.3 | scaffold installable as NS/NAME 0.1.0 | authoring › init creates a package with its first release | covered |  |
| 004.4 | missing identity: repository-local fallback only | authoring › Git identity is never invented in existing repositories | covered |  |
| 004.5 | existing directory with a sentinel is preserved | authoring › init into a multi-package repository › Existing folder that is not a repository | covered | cargo `scaffold::init_never_overwrites_and_validates_ids`; no Dagger case |
| 004.6 | no origin, catalog entry or installation created | cli › Nothing is built, executed or uploaded | covered | requirement level |
| 005.1 | interactive init: reprompt, EOF, cancellation | package-ids › No default namespace | obsolete | `init NS/NAME --into DIR` never prompts (itests README "Not ported" uc_005) |
| 006.1 | version, commit and scoped tag agree; all non-ignored changes committed | authoring › commit releases a version | covered |  |
| 006.2 | no build, push or catalog mutation | authoring › commit releases a version; cli › Nothing is built, executed or uploaded | covered |  |
| 006.3 | existing version/tag rejected without partial mutation | authoring › Existing tags are never moved | covered |  |
| 006.4 | malformed, pre-release, build-metadata versions rejected | authoring › commit releases a version › Invalid version; package-ids › Version format | covered | `pre-release-versions` (active) would accept pre-release |
| 007.1 | origin fetch and push URLs replaced | authoring › remote sets origin without network › Set origin | covered |  |
| 007.2 | other remotes survive, nothing pushed | authoring › remote sets origin without network › Other remotes are kept | missing → added | tagged `uc_007:set_then_replace_origin` |
| 007.3 | non-repository / repository without plugin.json fails | authoring › remote sets origin without network › Not a package repository root | covered |  |
| 008.1 | auth check: nonempty and empty bare repos succeed without cloning | git-safety › Read-only repository access check | covered |  |
| 008.2 | missing path / invalid source fail with sanitized diagnostics | git-safety › Read-only repository access check › Invalid input | covered |  |
| 008.3 | workspace and catalog unchanged | git-safety › Read-only repository access check | covered |  |
| 009.1 | credentials in HTTP userinfo rejected | git-safety › Credentials in URLs are the user's responsibility › Token in URL | covered |  |
| 009.2 | markers in URL query/fragment rejected, not leaked | git-safety › Credentials in URLs are the user's responsibility › Query or fragment in an HTTP URL | missing → added | cargo `auth::rejects_embedded_credentials_before_starting_git`; Dagger `uc_009:rejects_https_query_token` etc. (loop-tagged `#token-in-url`) |
| 009.3 | unavailable endpoint fails in bounded time | git-safety › Git never waits for input; git-safety › Read-only repository access check | covered |  |
| 009.4 | raw git stderr not echoed | git-safety › Read-only repository access check › Unknown host key | covered |  |
| 009.5 | no terminal prompt; strict noninteractive default SSH | git-safety › Git never waits for input › Access check ls-remote / Credential prompt | covered | `robustness-fixes` (TILR-53) switches auth check to `StrictHostKeyChecking=accept-new` |
| 009.6 | explicit SSH overrides kept | git-safety › Git never waits for input › User's GIT_SSH_COMMAND / Probe of the user's SSH setting | covered |  |
| 010.1 | install by exact scoped tag | install-uninstall › Install an explicit ref or the dev channel | covered |  |
| 010.2 | install by exact SHA | install-uninstall › Install an explicit ref or the dev channel › Install by exact commit | covered |  |
| 010.3 | uncommitted source edits not installed, source unchanged | dist › Deploys copy the committed tree › Uncommitted edits are not deployed | covered |  |
| 010.4 | repeat install of the same ID is rejected | install-uninstall › Install is idempotent for the same source › Repeat install | obsolete | install is idempotent since 0.5 (`2026-10-17-unified-cli-output`): same source is a no-op |
| 010.5 | same ID from another source rejected without overwrite | install-uninstall › Same ID from another source is refused › Other source | covered |  |
| 011.1 | stable picks numeric max 1.10.0 (annotated + lightweight, reverse order) | install-uninstall › Install the latest release of a Git source › Newest numeric tag wins; package-ids › Version format › Numeric ordering | covered |  |
| 011.2 | untagged HEAD ignored | install-uninstall › Install the latest release of a Git source › Newest numeric tag wins | covered |  |
| 011.3 | repository without release tags fails | install-uninstall › Install the latest release of a Git source › Repository without releases | covered |  |
| 012.1 | --channel dev installs default-branch HEAD | install-uninstall › Install an explicit ref or the dev channel | covered | `single-ref-mode` (D6, TILR-40) replaces `--channel` (`--dev`) |
| 012.2 | update --channel dev follows origin default branch | updates › Dev channel follows the origin HEAD by commit | covered |  |
| 012.3 | channel chosen per invocation, not persisted | — (single-ref-mode › mode recorded in state) | design-pending | `single-ref-mode` (D6, TILR-40) records the mode; main specs do not state "not persisted" |
| 012.4 | explicit ref selects its revision | updates › Explicit ref resolves against the origin › Branch name | covered |  |
| 012.5 | --ref with --channel and unknown refs fail without partial install/update | cli › Option validation › Channel and ref; errors › Failing operations leave the workspace unchanged | covered |  |
| 013.1 | same basename under different namespaces coexists | package-ids › IDs are matched exactly; workspace-layout › Workspace folders | covered |  |
| 013.2 | state, clones and dist isolated by ID and --root; others byte-identical | transactions-locking › Transactions stage only touched paths; transactions-locking › Neighbouring folders are never touched | covered |  |
| 014.1 | malformed / missing plugin.json fails with actionable error | package-manifest › Package folder contents › Missing plugin.json; package-manifest › Known manifest keys | covered |  |
| 014.2 | invalid ID / version in manifest | package-ids › Package ID syntax; package-ids › Version format | covered |  |
| 014.3 | missing dist | dist › A revision without dist is refused › Deploy from Git objects | covered |  |
| 014.4 | empty dist (unspecified contract) recorded | dist › A revision without dist is refused | covered | empty dist counts as missing |
| 014.5 | conflicting IDs | install-uninstall › Same ID from another source is refused; package-ids › Case-colliding IDs are refused | covered |  |
| 014.6 | path escapes / symlinks in dist | dist › Deploys copy the committed tree › Symlink inside dist; install-uninstall › Install from the catalog › Tampered dist | covered |  |
| 014.7 | catalog installs/registers of broken manifests | install-uninstall › Install from the catalog; catalog › Register one version | covered |  |
| 014.8 | healthy sentinel plugin untouched, no partial entry | errors › Failing operations leave the workspace unchanged | covered |  |
| 015.1 | update with no catalog configured; catalog never read | updates › Updates need no catalog › Catalog-less update | covered |  |
| 015.2 | repeat update is a no-op | updates › Stable update moves to the newest release › Update and repeat | covered |  |
| 015.3 | stable ignores untagged HEAD | updates › Stable update moves to the newest release | covered |  |
| 015.4 | latest release older than installed → version_conflict | updates › Stable update never downgrades or accepts mutated releases; updates › Update mirrors the origin's tags › Deleted installed release tag | covered | tag policing removal decided: `no-tag-immutability` (D5, TILR-39): retracted release → up to date + warning |
| 015.5 | installed version retagged → version_conflict | updates › Stable update never downgrades or accepts mutated releases › Release commit of the installed version moved | covered | tag policing removal decided: `no-tag-immutability` (D5, TILR-39) |
| 016.1 | update by tag / SHA to 0.1.1 | updates › Explicit ref resolves against the origin | covered |  |
| 016.2 | explicit older revision allowed | updates › Explicit ref resolves against the origin | covered |  |
| 016.3 | same version, different commit rejected | updates › Explicit ref resolves against the origin › Same version on another commit | covered |  |
| 016.4 | plugin ID change rejected without replacing state/dist | errors › Failing operations leave the workspace unchanged; package-ids › Broken historical tags are skipped › Bad newest tag | covered | `--ref`: "plugin id changed upstream"; newest tag with a foreign ID: `version_conflict` "latest release tag is broken" |
| 016.5 | moved already-fetched release tag rejected | package-ids › Moved release tags are refused › Retagged release | covered | tag policing removal decided: `no-tag-immutability` (D5, TILR-39) |
| 016.6 | late red: tag with a foreign package ID silently skipped | package-ids › Release tag names; package-ids › Broken historical tags are skipped › One bad old tag | covered | install warns, newest is refused; `update` prints no warning — Defect D1 |
| 017.1 | rollback without history fails safely | updates › Rollback swaps current and previous | covered | `no_previous_version` |
| 017.2 | one previous version; rollback swaps current/previous | workspace-layout › Installed package record › Update keeps one previous revision; updates › Rollback swaps current and previous › Rollback after update | covered |  |
| 017.3 | enabled state kept across rollback | dist › Inactive packages change state and clone only | covered |  |
| 017.4 | missing / corrupt previous snapshot fails, active intact | errors › Failing operations leave the workspace unchanged; dist › A revision without dist is refused | covered |  |
| 018.1 | update/rollback refuse on tracked, untracked, local commit, stash | updates › Local work blocks update and rollback › Stash blocks update | covered |  |
| 018.2 | uninstall without --force, no TTY: refused | install-uninstall › CLI confirmation flow for uninstall › No TTY | covered |  |
| 018.3 | TTY prompt lists the local work; N/EOF change nothing; y removes | install-uninstall › CLI confirmation flow for uninstall; install-uninstall › Uninstall is planned and confirmed › Confirmed uninstall; cli › Prompts only on a TTY | covered |  |
| 018.4 | uninstall --force removes without prompt | install-uninstall › CLI confirmation flow for uninstall | covered |  |
| 018.5 | author source and unrelated plugin byte-identical | install-uninstall › Only manager-owned files are deleted | covered |  |
| 019.1 | failed update (missing dist, bad manifest, mismatched ID, bad ref) preserves state/dist/rollback | errors › Failing operations leave the workspace unchanged; dist › A revision without dist is refused › Release without dist | covered |  |
| 019.2 | origin unavailable | errors › Failing operations leave the workspace unchanged › Network failure mid-update | covered |  |
| 019.3 | later valid update succeeds | updates › Stable update moves to the newest release | covered |  |
| 020.1 | disable/enable change only plugins.A.enabled in state | dist › Enable and disable deploy and remove › Disable and enable | covered |  |
| 020.2 | dist of A byte-identical across disable | dist › Enable and disable deploy and remove | obsolete | since 0.5 dist holds only active packages (`2026-10-17-dist-active-only`): disable removes `dist/A` |
| 020.3 | repeated disable/enable is a no-op | dist › Enable and disable deploy and remove | covered |  |
| 020.4 | unknown ID → not_installed | errors › Stable error kinds › Kind of a missing package | covered |  |
| 020.5 | update of a disabled plugin keeps enabled:false | dist › Inactive packages change state and clone only | covered |  |
| 020.6 | update of a disabled plugin deploys the new dist | dist › Inactive packages change state and clone only | obsolete | since 0.5 dist holds only active packages (`2026-10-17-dist-active-only`) |
| 021.1 | forget removes state only, retains clone | install-uninstall › Three removal levels | covered | dist is removed since 0.5 (`2026-10-17-dist-active-only`) |
| 021.2 | restore recreates state without old rollback history | install-uninstall › Restore preconditions › Restored record | covered | `single-ref-mode` (TILR-59) will re-record revisions on restore |
| 021.3 | unknown ID, duplicate restore, missing clone, malformed manifest fail safely | errors › Stable error kinds › Removal of an unknown package; install-uninstall › Restore preconditions; install-uninstall › Three removal levels › Restore without a clone | covered |  |
| 022.1 | uninstall clean plugin: exact removal, no prompt | install-uninstall › Uninstall is planned and confirmed; install-uninstall › Only manager-owned files are deleted | covered |  |
| 022.2 | namespace-directory uninstall refused with/without --force | package-ids › Package ID syntax; errors › Stable error kinds › Removal of an unknown package | covered |  |
| 022.3 | unknown ID error names the ID, no change | cli › Errors and exit codes › Unknown ID; errors › Errors name the package | covered |  |
| 022.4 | NS/P with only ns/p installed (case-insensitive FS) is unknown | package-ids › IDs are matched exactly › Case variant on APFS | covered | Dagger BLOCKED on Linux; cargo `uninstall::case_variant_id_never_touches_the_installed_plugin` |
| 022.5 | fork author repository outside the workspace survives | install-uninstall › Only manager-owned files are deleted › Uninstalling a fork | covered |  |
| 023.1 | fork keeps history and tracked/untracked edits | fork-replacement › Fork copies an installed package | covered |  |
| 023.2 | fork ID NS/name regardless of folder; version kept; forkOf recorded | fork-replacement › Fork metadata › Fork manifest | covered |  |
| 023.3 | original installation unchanged | fork-replacement › Fork copies an installed package › Fork one package | covered |  |
| 023.4 | remotes: original becomes upstream | fork-replacement › Fork copies an installed package | covered |  |
| 024.1 | unique fork branch (fork, fork-2) | fork-replacement › Fork metadata | covered |  |
| 024.2 | author/vendor scoped tags do not conflict; vendor tags not fork releases | package-ids › Release tag names › Scoped tag | covered |  |
| 024.3 | fork installs and updates independently, no automatic replacement | fork-replacement › Replacement is explicit and global; package-manifest › forkOf is provenance, replaces is an offer | covered |  |
| 025.1 | same-ID / same-namespace fork refused | fork-replacement › Fork requires explicit namespace and destination › Same namespace | covered |  |
| 025.2 | unknown plugin refused | fork-replacement › Fork copies an installed package | covered | only installed IDs |
| 025.3 | existing destination, source path, managed workspace paths refused unchanged | fork-replacement › Fork requires explicit namespace and destination › Destination inside the workspace | covered |  |
| 025.4 | legacy-to-scoped fork records legacy provenance | fork-replacement › Fork metadata › Fork manifest; package-ids › Release tag names › Legacy tag | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 026.1 | legacy manifest and v-tags usable for install/update/register | package-ids › Release tag names › Legacy tag; catalog › Release record › Legacy release without hashes | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 026.2 | legacy and scoped IDs coexist | package-ids › Package ID syntax | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 026.3 | prefix collisions rejected in both orders | package-ids › Package ID syntax › Legacy ID equal to an installed namespace | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 027.1 | register a committed scoped tag into a new catalog | catalog › Register one version | covered |  |
| 027.2 | identical registration is safe | catalog › Published versions are immutable | covered |  |
| 027.3 | other commit same version / other source same ID conflict | catalog › Published versions are immutable | covered |  |
| 027.4 | hashes describe committed content only | catalog › Release record | covered |  |
| 028.1 | schema 2 tree and relative links resolve | catalog › Catalog v2 is a tree of static files | covered |  |
| 028.2 | manifest, source, commit, reference, file digests and sizes match Git | catalog › Release record | covered |  |
| 028.3 | adding a release preserves previous records and other plugins | catalog › Published versions are immutable | covered |  |
| 029.1 | install exact and numeric latest at the registered commit | install-uninstall › Install from the catalog | covered |  |
| 029.2 | tampered manifest, size, hash, commit refused before any change | install-uninstall › Install from the catalog › Tampered dist | covered | `protected-catalog` (D4, TILR-34) limits hash checks to verified catalogs |
| 029.3 | unknown ID / version fail cleanly | install-uninstall › Install from the catalog; errors › Errors name the package | covered |  |
| 030.1 | paired HTTPS build URL + SHA-256 stored | catalog › Register one version | covered |  |
| 030.2 | single field, malformed hash, non-HTTPS rejected | catalog › Register one version › Invalid build metadata | covered |  |
| 030.3 | changing build metadata of a release rejected | catalog › Published versions are immutable › Register with another build | covered |  |
| 030.4 | install uses Git, ignores an unreachable build URL | catalog › Release record; cli › Nothing is built, executed or uploaded | covered |  |
| 031.1 | all valid releases indexed once with committed hashes | catalog › Catalog generated from Git release tags › Two releases | covered |  |
| 031.2 | comments, blanks, paths relative to the sources file, other cwd | catalog › Catalog generated from Git release tags | covered |  |
| 031.3 | duplicate source entries (other spellings) read once | catalog › Catalog generated from Git release tags › Duplicate source lines | missing → added | tagged `uc_031:all_releases_indexed_once` |
| 031.4 | repeat is safe, no build/push | catalog › Published versions are immutable › Regenerate | covered |  |
| 032.1 | source validation precedes writes; no partial publish | catalog › Catalog generated from Git release tags; catalog › Writers are serialized and crash-safe | covered |  |
| 032.2 | malformed / mismatched newest tag in a second source | package-ids › Broken historical tags are skipped › Bad newest tag | covered |  |
| 032.3 | moved old release refused | package-ids › Moved release tags are refused › Moved tag on regenerate | covered | tag policing removal decided: `no-tag-immutability` (D5, TILR-39): keep record + warn |
| 032.4 | retry succeeds; removed sources and build metadata retained | catalog › Published versions are immutable › Regenerate; catalog › Catalog generated from Git release tags | covered |  |
| 033.1 | latest = 1.10.0 (numeric) | package-ids › Version format › Numeric ordering | covered |  |
| 033.2 | old mismatched tag skipped with a warning naming it | package-ids › Broken historical tags are skipped › One bad old tag | covered | install and catalog-update warn; update does not — Defect D1 |
| 033.3 | broken tag newer than every valid release fails, nothing written | package-ids › Broken historical tags are skipped › Bad newest tag | covered |  |
| 033.4 | fork releases only by the fork ID's tags | package-ids › Release tag names | covered |  |
| 033.5 | pre-release, build and non-numeric tags ignored | package-ids › Release tag names › Tags that are not release versions | missing → added | tagged `uc_033:prerelease_build_and_named_tags_ignored`; `pre-release-versions` (active) would make `-beta` tags releases |
| 033.6 | manifest version "1.0" via --ref refused | package-ids › Version format › Pre-release suffix is refused | covered |  |
| 034.1 | info prints address, format, per-plugin ID, latest, version count, source; read-only | catalog › Bounded overview › Overview lines | missing → added | tagged `uc_034:info_local_root`; mirrored into `cli-catalog-commands` (`catalog show`) |
| 034.2 | info shows install/check catalog separately; PM_REMOTE_CATALOG / PM_PUBLIC_URL selection; Russian text | — | obsolete | one catalog setting, no default/remote/public catalog (`2026-10-16-gitpm-cli`; catalog › One catalog per workspace) |
| 034.3 | `catalog` prints only the current level's links | cli › Command set › Catalog links | covered |  |
| 034.4 | branch path shows only that branch | catalog › Catalog v2 is a tree of static files › Direct lookup | covered |  |
| 034.5 | loopback http works, non-loopback http refused | catalog › Catalog locations and transport › Plain HTTP to a remote host | covered |  |
| 034.6 | >40 entries: limit and hint | catalog › Bounded overview › Large catalog | covered |  |
| 034.7 | v1 catalog readable | catalog › v1 catalogs stay readable | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 035.1 | statuses reflect state vs catalog without updating | catalog › check compares installed with the catalog › Statuses | covered |  |
| 035.2 | reads only installed plugins' entries, survives broken unrelated indexes | catalog › check compares installed with the catalog; catalog › Catalog v2 is a tree of static files › Direct lookup | covered |  |
| 035.3 | every status branch | catalog › check compares installed with the catalog › Statuses / Identity mismatch / Broken payload / Other catalog | covered |  |
| 036.1 | base URL and full catalog.json URL read the same | catalog › Catalog locations and transport | covered |  |
| 036.2 | 404 of a plugin → not in catalog | catalog › Catalog locations and transport › Failed request | covered |  |
| 036.3 | malformed, truncated JSON, unsupported schema | catalog › Catalog v2 is a tree of static files › Unknown schema; errors › Stable error kinds | covered |  |
| 036.4 | server down → http, bounded | catalog › Catalog locations and transport › Failed request; git-safety › Git never waits for input › Catalog download | covered |  |
| 036.5 | 301 not followed | catalog › Catalog locations and transport | covered |  |
| 036.6 | announced body > 16 MiB refused | catalog › Catalog locations and transport › Failed request | covered |  |
| 036.7 | streamed body > 16 MiB refused | catalog › Catalog locations and transport › Streamed body without a length | design-pending | `catalog-streaming-size-limit`; `uc_065` case is XFAIL |
| 036.8 | traversal, absolute and foreign links refused | catalog › Catalog v2 is a tree of static files; catalog › Bounded overview | covered |  |
| 036.9 | register to HTTP catalog refused; non-loopback http refused | catalog › Catalog locations and transport › Plain HTTP to a remote host | covered |  |
| 036.10 | workspace byte-identical after failing reads | errors › Failing operations leave the workspace unchanged | covered |  |
| 037.1 | read/install a v1 catalog before migration | catalog › v1 catalogs stay readable | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 037.2 | migrate keeps backup and old releases, invents no hashes | catalog › v1 catalogs stay readable › Migrate | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 037.3 | repeated migration is safe | catalog › v1 catalogs stay readable › Migrate a v2 catalog | missing → added | tagged `uc_037:repeated_migration_is_safe`; legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 037.4 | register auto-migrates; new registrations carry hashes | catalog › v1 catalogs stay readable › Register into a v1 catalog | missing → added | tagged `uc_037:register_auto_migrates`; legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 037.5 | backup excluded from publication | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 038.1 | sync dry-run, upload order, validation, local-only sources | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 039.1 | sync upload failures, purge, retry, --publish | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 040.1 | held workspace lock refuses a second mutation; retry after release | transactions-locking › Workspace lock › Two CLI processes | covered |  |
| 040.2 | lock of a killed owner does not block | transactions-locking › Locks are process-owned and survive crashes › Killed owner | covered |  |
| 040.3 | interrupted transaction recovered on next open | transactions-locking › Crash recovery › Crash during commit; transactions-locking › Transactions stage only touched paths › Failure before commit | covered |  |
| 040.4 | separate catalog writer lock | catalog › Writers are serialized and crash-safe › Concurrent catalog-update | covered |  |
| 041.1 | symlinks / FIFO in payload refused without following or hanging | dist › Deploys copy the committed tree › Symlink inside dist; errors › Failing operations leave the workspace unchanged | covered |  |
| 041.2 | repository symlink outside dist kept, not followed | transactions-locking › Managed paths are never followed through symlinks | covered |  |
| 041.3 | traversal and absolute IDs refused | package-ids › Package ID syntax › Unsafe components are refused | covered |  |
| 041.4 | namespace deletion and symlinked namespace dir never followed | install-uninstall › Only manager-owned files are deleted; transactions-locking › Managed paths are never followed through symlinks › Symlinked namespace | covered |  |
| 041.5 | FIFO / symlink in catalog handled by sync | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 042.1 | each commit = one commit + ID@V tag, canonical manifest, clean tree | authoring › commit releases a version; package-manifest › Manifest output format | covered |  |
| 042.2 | catalog-update per version, idempotent, old releases byte-identical | catalog › Catalog generated from Git release tags; catalog › Published versions are immutable › Regenerate | covered |  |
| 042.3 | sync --dry-run lists reachable files in order | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 042.4 | fork release has forkOf/replaces and its own catalog entry | fork-replacement › Fork metadata; package-manifest › forkOf is provenance, replaces is an offer | covered |  |
| 043.1 | ignored, staged, untracked, stash, unpublished refs: update/rollback/uninstall (no TTY) refuse | updates › Local work blocks update and rollback; install-uninstall › CLI confirmation flow for uninstall › No TTY | covered |  |
| 043.2 | TTY prompt lists each item; N refuses | install-uninstall › CLI confirmation flow for uninstall; install-uninstall › Uninstall is planned and confirmed | covered |  |
| 043.3 | fork of each dirty clone preserves ignored and unpublished work | fork-replacement › Fork copies an installed package | covered |  |
| 043.4 | confirmed / forced uninstall removes only the fixture | install-uninstall › Uninstall is planned and confirmed › Confirmed uninstall; install-uninstall › Only manager-owned files are deleted | covered |  |
| 044.1 | forkOf.commit = local HEAD, forkOf.version = HEAD manifest | fork-replacement › Fork metadata | covered |  |
| 044.2 | identity commit only plugin.json; staged/untracked/ignored survive | fork-replacement › Fork copies an installed package; fork-replacement › Fork metadata | covered | cargo `fork_records_local_head_preserves_index_and_immediate_parent` |
| 044.3 | fork of fork: immediate parent | fork-replacement › Fork metadata | covered |  |
| 044.4 | provenance kept through catalog, update, rollback | fork-replacement › Fork metadata; fork-replacement › Active fork rules | covered |  |
| 045.1 | truncated / bad-ID / wrong-typed state.json: every command fails, never rewritten | workspace-layout › Foreign directories are refused › Corrupt state; errors › Conversions choose documented kinds | covered |  |
| 045.2 | removed clone: commands on A fail, B unaffected | dist › Undeployable packages do not block others; install-uninstall › Three removal levels › Restore without a clone | covered |  |
| 045.3 | removed dist recreated on update / restore | dist › Every mutating command reconciles all of dist; dist › Forget, restore and uninstall settle dist | covered |  |
| 045.4 | moved HEAD: update refused, uninstall (no TTY) lists HeadMoved | updates › Local work blocks update and rollback; install-uninstall › Uninstall is planned and confirmed | covered |  |
| 045.5 | leftover uncommitted .pm-txn discarded | transactions-locking › Transactions stage only touched paths › Failure before commit | covered |  |
| 045.6 | restored snapshot updates | updates › Stable update moves to the newest release | covered |  |
| 046.1 | denied write at stage copy / state write changes nothing | transactions-locking › Transactions stage only touched paths › Failure before commit | covered |  |
| 046.2 | activation failure / kill after staging recovered | transactions-locking › Crash recovery › Crash during commit | covered |  |
| 046.3 | catalog write failure then retry keeps records | catalog › Writers are serialized and crash-safe | covered |  |
| 047.1 | list, disable/enable, rollback, forget/restore, uninstall offline | catalog › A catalog is optional › Lifecycle without a catalog; updates › Rollback swaps current and previous | covered |  |
| 047.2 | update fails cleanly offline, then succeeds | errors › Failing operations leave the workspace unchanged › Network failure mid-update | covered |  |
| 047.3 | fork and original independent | package-manifest › forkOf is provenance, replaces is an offer › Installing a fork changes no consumer | covered |  |
| 048.1 | disable → update → rollback | dist › Inactive packages change state and clone only; updates › Rollback swaps current and previous | covered |  |
| 048.2 | forget → install same ID refused; retained clone never overwritten | install-uninstall › Same ID from another source is refused › Forgotten but retained | covered |  |
| 048.3 | dirty clone → forget → restore fails without loss | install-uninstall › Restore preconditions › Dirty retained clone | covered |  |
| 048.4 | forgotten clean clone → restore → uninstall | install-uninstall › Three removal levels › Forget then restore | covered |  |
| 048.5 | fork disabled while original enabled and inverse | dist › Enable and disable deploy and remove; package-manifest › forkOf is provenance, replaces is an offer | covered |  |
| 048.6 | restore resets enabled=true, previous=None | install-uninstall › Restore preconditions › Restored record | covered | `single-ref-mode` (TILR-59) re-records `enabled` and revisions |
| 049.1 | credential helper: auth check, install, update succeed | git-safety › Credentials in URLs are the user's responsibility | covered | optional infrastructure (loopback git http-backend) |
| 049.2 | revoked: update fails in bounded time without prompt, unchanged | git-safety › Git never waits for input › Credential prompt; errors › Failing operations leave the workspace unchanged | covered |  |
| 049.3 | credentials never in state, catalog or output | git-safety › Credentials in URLs are the user's responsibility | covered | helper credentials only; URL-embedded ones are recorded by design (› Credential URL given to install) |
| 050.1 | concurrent same-workspace installs/updates serialize or lock, no lost state | transactions-locking › Workspace lock › Two CLI processes | covered |  |
| 050.2 | concurrent same-catalog registrations | catalog › Writers are serialized and crash-safe | covered |  |
| 050.3 | independent workspaces in parallel | transactions-locking › Workspace lock | covered |  |
| 051.1 | publish under failure (sync phases, purge, stale index) | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests`; consumer side re-expressed as `uc_077` (catalog › Catalog locations and transport) |
| 052.1 | library and CLI give identical state, provenance, edges, dist, error kinds | install-uninstall › Three removal levels; fork-replacement › Replacement is explicit and global; dependencies › Dependents guard removal; errors › Stable error kinds | covered | `gitpm-api` parity cases in `uc_052` |
| 052.2 | no business rule exists only in the CLI | — | design-pending | `thin-cli-boundary` (D2, TILR-29/31/32/33/22/48) moves the remaining CLI-only logic into the library |
| 053.1 | provenance alone redirects no consumer | fork-replacement › Replacement is explicit and global; package-manifest › forkOf is provenance, replaces is an offer › Installing a fork changes no consumer | covered |  |
| 053.2 | inspectable global binding; both consumers get the fork payload | fork-replacement › Replacement is explicit and global › Replace for all consumers; dist › Replace removes the original's dist | covered |  |
| 053.3 | original stays installed and addressable; unrelated unchanged | dist › dist holds exactly the active packages › Enabled but replaced | covered |  |
| 053.4 | new consumer after replacement resolves to the fork | fork-replacement › Replacement is explicit and global | covered |  |
| 053.5 | binding persists after reopen | workspace-layout › Replacements and forgotten records | covered |  |
| 054.1 | compatible fork replaces without prompt, transitive consumer too | fork-replacement › Compatibility is checked by versions only; fork-replacement › Replacement is explicit and global › Replace for all consumers | covered |  |
| 054.2 | incompatible on a TTY: warning names app/b; N/EOF nothing; y switches all | cli › Prompts only on a TTY › Replacement prompt declined; fork-replacement › Compatibility is checked by versions only | covered |  |
| 054.3 | no TTY refused; --force applies with warning | fork-replacement › Compatibility is checked by versions only › Incompatible consumer without a TTY | covered |  |
| 054.4 | self and cyclic replacement refused, not overridable | fork-replacement › Self-replacement and cycles are refused › Cycle | covered |  |
| 054.5 | two forks: one global binding | fork-replacement › Replacement is explicit and global › Output variants; fork-replacement › Switching back | covered |  |
| 054.6 | forkOf never triggers replacement | fork-replacement › Replacement is explicit and global | covered |  |
| 055.1 | original release does not switch consumers back | dist › Inactive packages change state and clone only › Update of a replaced package; fork-replacement › Active fork rules | covered |  |
| 055.2 | fork update keeps binding; rollback follows fork revision | fork-replacement › Active fork rules | covered |  |
| 055.3 | incompatible fork release, unavailable origin, corrupt payload keep binding | dependencies › Updates keep dependents working; errors › Failing operations leave the workspace unchanged | covered |  |
| 055.4 | reopen and recheck | workspace-layout › Replacements and forgotten records | covered |  |
| 056.1 | disable/forget/uninstall of active provider refused naming consumers | dependencies › Dependents guard removal | covered | `removal-and-restore-rules` (D14/D16, TILR-55/56/58/60) changes disable/forget of an active fork |
| 056.2 | switch back to compatible original: all consumers return, fork work kept | fork-replacement › Switching back; dist › Switching back redeploys the original › Back to the original | covered |  |
| 056.3 | switch back to incompatible original: warn and confirm | fork-replacement › Switching back; cli › Prompts only on a TTY | covered |  |
| 056.4 | switch back to unavailable original fails, binding kept | fork-replacement › Switching back; fork-replacement › Active fork rules › Reinstalled original stays replaced | covered | `removal-and-restore-rules` (TILR-55/56/58/60) revises `--back` after the original was uninstalled |
| 056.5 | remove unused fork; dirty fork follows uninstall contract | install-uninstall › Only manager-owned files are deleted › Uninstalling a fork; install-uninstall › Uninstall is planned and confirmed › Confirmed uninstall | covered |  |
| 057.1 | originating catalog stored in state | catalog › One catalog per workspace | covered |  |
| 057.2 | check against catalog B reports a different source | catalog › check compares installed with the catalog › Other catalog / Identity mismatch | covered |  |
| 057.3 | update does not switch to B's source | updates › Updates need no catalog; catalog › One catalog per workspace | covered |  |
| 057.4 | installing from B while A's package is installed refused | catalog › One catalog per workspace › Second catalog | covered |  |
| 057.5 | same commit in A and B: up to date | catalog › check compares installed with the catalog › Other catalog | covered |  |
| 057.6 | info prints both addresses, PM_PUBLIC_URL fallback | — | obsolete | one catalog setting, no default/remote/public catalog (`2026-10-16-gitpm-cli`; catalog › One catalog per workspace) |
| 057.7 | --catalog does not change the R2 publish destination | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 058.1 | dependency validation: banana, pre-release, self, invalid ID | dependencies › Requirement syntax › Self-dependency / Pre-release requirement | covered | `pre-release-versions` (active) changes requirement syntax |
| 058.2 | install-catalog pulls lib/core 1.1.0; edges in state; list resolved | dependencies › Install resolves dependencies › Dependency pulled from the catalog; dependencies › State keeps dependency edges › list after install | covered |  |
| 058.3 | missing dependency refused naming it | dependencies › Conflicts are explained | covered |  |
| 058.4 | version conflict names both consumers | dependencies › Conflicts are explained › Two consumers disagree | covered |  |
| 058.5 | cycle resolves deterministically in time | dependencies › Conflicts are explained | covered | cycles between installed packages resolve normally |
| 058.6 | update lib/core to 2.0.0 refused naming app/a | dependencies › Updates keep dependents working › Breaking update | covered |  |
| 058.7 | shared dependency single copy; uninstall app/a keeps it | dependencies › Install resolves dependencies; dependencies › State keeps dependency edges | covered |  |
| 058.8 | direct Git install with dependencies: no catalog refused; installed / catalog succeed | dependencies › Install resolves dependencies › No catalog | covered |  |
| 059.1 | uninstall lib/core refused naming app/a, app/b | dependencies › Dependents guard removal › Uninstall a dependency | covered |  |
| 059.2 | forget and disable guarded the same way | dependencies › Dependents guard removal | covered | `download-and-enable` (D7, TILR-36) revises disabled dependencies |
| 059.3 | uninstall --force: dependents reported unsatisfied | dependencies › Broken consumers are reported, not enforced › Unrelated install after forced removal | covered |  |
| 059.4 | unused dependency uninstalls without guard | dependencies › Dependents guard removal | covered |  |
| 059.5 | dirty dependency: guard plus lost-work confirmation | dependencies › Dependents guard removal; install-uninstall › Uninstall is planned and confirmed | covered |  |
| 060.1 | list shows forkOf/replaces for the fork, none for the original | fork-replacement › Fork metadata; cli › JSON output for state and catalog › list | covered |  |
| 060.2 | check compares fork with its own releases; original line under the table | fork-replacement › check shows fork upstream | covered |  |
| 060.3 | fork of fork points to the immediate parent | fork-replacement › Fork metadata | covered |  |
| 060.4 | newer original release visible; fork unchanged | fork-replacement › check shows fork upstream › Original has a newer release | covered |  |
| 061.1 | 80-char component accepted, 81 refused, nothing created | package-ids › Package ID syntax; install-uninstall › Refused input creates nothing | covered |  |
| 061.2 | non-ASCII/NFD, '.', '..', space, leading '-' refused | package-ids › Package ID syntax › Unsafe components are refused | covered |  |
| 061.3 | case variants (Dan/p, Dan/q, after forget, fork into Dan) refused | package-ids › Case-colliding IDs are refused › Install after forget of a case variant | covered |  |
| 061.4 | bad historical tag skipped with warning (install, catalog-update) | package-ids › Broken historical tags are skipped › One bad old tag | covered |  |
| 061.5 | mismatched tag below latest skipped with warning | package-ids › Release tag names › Tag disagrees with the manifest | covered |  |
| 061.6 | unknown manifest field → invalid_manifest | package-manifest › Unknown keys are preserved | obsolete | unknown keys are kept since `2026-10-16-rename-to-gitpm-and-crates-io` |
| 061.7 | tag off the default branch works by tag | install-uninstall › Install an explicit ref or the dev channel; catalog › Catalog generated from Git release tags | covered |  |
| 061.8 | scoped ID with vX.Y.Z tag accepted | package-ids › Release tag names › vVERSION tag of a scoped single-package repository | covered | legacy IDs/v1 removal decided: `no-backward-compat` (D1, TILR-38) |
| 061.9 | invalid manifest versions via --ref / --channel dev refused, no clone/dist/state entry | package-ids › Version format; install-uninstall › Refused input creates nothing | covered |  |
| 061.10 | 200 versions: numeric max, each command < 60 s | package-ids › Version format › Numeric ordering; catalog › Bounded overview | covered |  |
| 062.1 | enable on enabled / disable on disabled: exit 0, state identical | dist › Enable and disable deploy and remove | covered |  |
| 062.2 | install of an installed ID: 'Already installed' error | install-uninstall › Install is idempotent for the same source › Repeat install | obsolete | install is idempotent since 0.5 (`2026-10-17-unified-cli-output`) |
| 062.3 | forget/uninstall again: unknown-ID error naming it | errors › Errors name the package › Repeated forget; errors › Stable error kinds › Removal of an unknown package | covered |  |
| 062.4 | restore of a registered ID: refused, state identical | install-uninstall › Restore preconditions | covered |  |
| 062.5 | register same release: exit 0, catalog identical | catalog › Published versions are immutable | covered |  |
| 062.6 | catalog-update unchanged sources: identical | catalog › Published versions are immutable › Regenerate | covered |  |
| 062.7 | migrate-catalog on a v2 catalog: no change | catalog › v1 catalogs stay readable › Migrate a v2 catalog | missing → added | tagged `uc_062:migrate_v2_catalog`; exit 0 checked by hand |
| 062.8 | commit existing version: tag exists, repo unchanged | authoring › Existing tags are never moved | covered |  |
| 062.9 | remote with the same URL: exit 0 | authoring › remote sets origin without network › Set origin | covered |  |
| 062.10 | namespace with the same value | — | obsolete | no namespace command (`2026-10-16-gitpm-cli`) |
| 062.11 | update with nothing new: no-op | updates › Stable update moves to the newest release › Update and repeat | covered |  |
| 063.1 | flags on the wrong command | cli › Option validation › Misplaced flag | covered | `--namespace`/`--publish` now unknown options |
| 063.2 | repeated flag, missing value, unknown flag/command/auth subcommand | cli › Option validation; cli › Command set › Removed command | covered |  |
| 063.3 | wrong argument count | cli › Option validation | covered |  |
| 063.4 | 'uninstall ID --force' = 'uninstall --force ID' | cli › Option validation › Flag position | missing → added | tagged `uc_063:uninstall_flag_order_is_irrelevant`; mirrored into the 5 changes that modify Option validation |
| 063.5 | --channel with --ref rejected | cli › Option validation › Channel and ref | covered | `single-ref-mode` (D6, TILR-40) replaces `--channel` |
| 063.6 | info boundary 40 / 41 entries, v2 > 40 | catalog › Bounded overview › Large catalog | covered |  |
| 063.7 | check colour: TTY vs pipe, NO_COLOR, TERM=dumb | catalog › check compares installed with the catalog › CLI table | covered |  |
| 063.8 | check exit code with updates available is stable | catalog › check compares installed with the catalog › Exit status | missing → added | tagged `uc_063:check_exit_code_without_updates`; mirrored into `check-via-origin` |
| 063.9 | --help / -h list every command | cli › Help; cli › Command set › Every command documented | covered |  |
| 064.1 | sync vs remote state: immutable remote releases, second machine, orphans, local sources, v1 backup | — | obsolete | gitpm never uploads (cli › Nothing is built, executed or uploaded); `sync`/R2 dropped in `2026-10-18-dagger-integration-tests` |
| 064.2 | stale catalog lock / temp file after a killed catalog-update | catalog › Writers are serialized and crash-safe | covered |  |
| 065.1 | symlink outside dist kept, all operations work; symlink inside dist refused | transactions-locking › Managed paths are never followed through symlinks; dist › Deploys copy the committed tree › Symlink inside dist | covered |  |
| 065.2 | update --ref BRANCH follows moved upstream; HEAD = origin HEAD; refs/heads refused | updates › Explicit ref resolves against the origin › Branch name | covered |  |
| 065.3 | non-loopback http refused, 301 not followed, announced 50 MB refused | catalog › Catalog locations and transport | covered |  |
| 065.4 | streamed 50 MB catalog response refused | catalog › Catalog locations and transport › Streamed body without a length | design-pending | `catalog-streaming-size-limit`; `uc_065:oversized_streamed_catalog_response_is_refused` XFAIL |
| 065.5 | namespaced release with empty/missing files refused; legacy may lack hashes | catalog › Release record › Namespaced release without hashes / Legacy release without hashes | covered |  |
| 065.6 | hanging transport and SSH passphrase fail fast; lock released | git-safety › Git never waits for input › Wedged git / Batch SSH command | covered |  |
| 065.7 | .<root>.stage / .<root>.backup neighbours never touched | transactions-locking › Neighbouring folders are never touched › Look-alike folder | covered |  |
| 065.8 | symlinked and real workspace path share the lock | transactions-locking › Workspace lock | covered |  |
| 065.9 | check/list never see partial state | transactions-locking › Transactions stage only touched paths › Readers during commit | covered | `thin-cli-boundary`: `list` stops taking the lock |
| 066.1 | publish base and consumers; install pulls vendor/base; list shows edges | dependencies › Install resolves dependencies › Dependency pulled from the catalog; dependencies › State keeps dependency edges | covered |  |
| 066.2 | fork, edit, release 1.0.1 replacing vendor/base, install | fork-replacement › Fork metadata › Fork manifest; authoring › commit releases a version | covered |  |
| 066.3 | replace without prompt; binding visible; fork payload active | fork-replacement › Replacement is explicit and global › Replace for all consumers; dist › Replace removes the original's dist | covered |  |
| 066.4 | new process: binding unchanged | workspace-layout › Replacements and forgotten records | covered |  |
| 066.5 | fork 1.0.2 update followed by consumers | fork-replacement › Active fork rules | covered |  |
| 066.6 | switch back: original payload, fork kept | fork-replacement › Switching back; dist › Switching back redeploys the original › Back to the original | covered |  |
| 066.7 | uninstall clean fork without dependents | install-uninstall › Only manager-owned files are deleted › Uninstalling a fork | covered |  |
| 067.1 | stand gate scripts/verify.py: every group passes | release-process › Required CI checks; integration-tests › One entry point runs the suite | obsolete | stand-only gate; replaced by `cargo test --workspace` CI and `./itests/dt` (`2026-10-18-dagger-integration-tests`) |
| 068.1 | install-catalog; list/check; state current/previous/enabled/catalog | install-uninstall › Install from the catalog; catalog › check compares installed with the catalog › CLI table | covered |  |
| 068.2 | catalog update alone changes nothing; check shows the update | catalog › check compares installed with the catalog | covered |  |
| 068.3 | update, rollback swaps current/previous | updates › Rollback swaps current and previous › Rollback after update | covered |  |
| 068.4 | disable / enable toggle the flag | dist › Enable and disable deploy and remove › Disable and enable | covered |  |
| 068.5 | forget / restore re-registers without history | install-uninstall › Three removal levels › Forget then restore | covered |  |
| 068.6 | late red: state.catalog survives forget/restore | install-uninstall › Restore preconditions › Restored record (today: no `catalog`) | design-pending | `single-ref-mode` (TILR-59): restore re-records `catalog`; today dropped (workspace.rs:1207-1215) |
| 068.7 | uninstall removes state/clone/dist; author repo and catalog identical | install-uninstall › Only manager-owned files are deleted | covered |  |
