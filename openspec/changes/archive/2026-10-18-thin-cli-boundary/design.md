# Design

## Context

The CLI (`crates/gitpm-cli/src/main.rs`) contains logic an FFI host would have to copy: catalog fallback (`catalog_for_install`-style path check), the `list` JSON (built in the CLI from `Manager::state` + `Manager::list`), the incompatible-replacement retry inside `fork_packages_replace` (calls `replace_plan`, prompts, then `replace`), `print_replacement`/`print_switch_back` reading `manager.state()`, `print_update_all` deriving "up to date" lines from pre-run state, and the `sync-dist` exit decision. Decision D2 (TILR-29..33) moves all of it into the library.

## API shapes

Library (`crates/gitpm`):

```rust
// workspace.rs
pub struct Options { pub catalog: Option<String> }        // validated by open_with
impl Manager {
    pub fn open_with(root: &Path, options: Options) -> Result<Manager>; // validates first, then creates
    pub fn fork_replace_resume(&self, pending: &[PendingReplacement], confirm: bool)
        -> ForkResume;   // { applied: Vec<ReplacementPlan>, failed: Option<ResumeFailure { original, fork, error }> }
}
pub struct PendingReplacement { pub original: String, pub fork: String, pub plan: ReplacementPlan }
pub struct ReplacementPlan {
    pub original: String, pub provider: String, pub version: String,
    pub current: Option<String>, pub consumers: Vec<String>,
    pub incompatible: Vec<IncompatibleConsumer>,
    pub installed_version: Option<String>,   // NEW: installed version of `provider` (FORK; ORIGINAL when switching back)
    pub fork_enabled: bool,                  // NEW
}
pub struct UpdateAllPlan { pub updates, pub installs, pub kept_back,
    pub unchanged: Vec<Unchanged> }         // NEW: {id, version}
pub struct InstallResult { …, pub dist: DistSync }        // NEW field on every mutating result
pub struct RevisionChange { …, pub skipped: Vec<SkippedTag>,   // NEW (TILR-103): broken tags skipped by update
                             pub dist: DistSync }
// PackageForkReplacement per package: failed: Option<ForkStep>, error, pending: Option<ReplacementPlan>

// list.rs (new, read-only, no Manager)
pub struct ListOptions { pub provides: Option<String> }
pub struct ListView { pub state: State, pub resolved: Vec<InstalledPackage> }
pub fn list_with(root: &Path, options: &ListOptions) -> Result<ListView>;

// scaffold.rs
pub struct Commit { pub package: String, pub version: String, pub tag: String }
pub fn commit_package(repo: &Path, version: &str, package: Option<&str>) -> Result<Commit>;

// error.rs
Error::NotDeployed { message: String, sync: Box<DistSync> }    // kind "not_deployed"
Error::HasDependents { …, replacing: bool }   // dependents are the originals of an active replacement
```

`Manager::sync_dist` returns `Ok(DistSync)` when nothing is skipped and `Err(NotDeployed { sync })` after committing otherwise. `sync_dist_plan` never fails for skipped packages.

FFI (`crates/gitpm-ffi`): free functions `list(root, provides: Option<String>) -> ListView`, `check(root, catalog: Option<String>) -> CheckReport`, `readState(root) -> ListView` (the full state view: raw state including `forgotten` and `replacements`, plus `resolved` with `active`; no filter), `catalogLinks(location) -> CatalogLinks`; `readInstalled` is removed (one free function for the state, no compatibility alias, D1); `PackageManager.check` removed; `PackageManager.forkReplaceResume(pending, confirm)`; records gain the fields above; `PmError.NotDeployed { message, sync: DistSync }` (the whole report, as the CLI prints it); `forkReplaceResume` returns `ForkResume { applied, failed: Option<ResumeFailure { original, fork, errorKind, errorMessage }> }`; `UpdateAllPlan.skipped`.

CLI hints: a table `kind → hint` in the CLI (`already_installed` for a forgotten ID → restore/uninstall; `invalid_argument` for a scaffold refusal → `init --multi`; `has_dependents` for an active replacement → `replace ORIGINAL --back`; HTTP catalog write → `catalog-update` needs a local folder). The hint is chosen from the kind and structured fields, never by parsing the message.

## Decisions

- `not_deployed` is a new error kind rather than an `Ok` result the CLI interprets: a host must not need to know that a non-empty `skipped` means failure. Alternative (Ok + CLI decides) rejected as CLI logic.
- `list` reads `state.json` without a lock: `state.json` is replaced by atomic rename, so a reader always sees a complete file; an interrupted transaction is completed by the next opening command. Alternative (shared lock) rejected: it would block hosts polling `list` during long installs.
- `--channel stable --ref x` becomes valid (stable is the default); only dev + ref is a library refusal. `single-ref-mode` removes `--channel` entirely later.
- `open_with` validates before creating; a failure after opening still leaves the empty workspace it created (documented in errors).

- `releases::latest` keeps `Discovery.skipped`, so `update`/`update all` return the skipped tags in `RevisionChange.skipped` (and, for `update all`, in the plan-level `UpdateAllPlan.skipped`, which covers updated, kept-back and unchanged packages) as install does in `InstallResult.skipped` (TILR-103).
- `readState` replaces `readInstalled`: both read `state.json` without opening; one function that returns the whole view is enough for hosts (restore offers need `forgotten`, status views need `resolved`).

## Transactions and crash recovery

No new write paths. `fork_replace_resume` runs one `replace` transaction per pending package, re-planning under the lock; a crash or a refusal between packages leaves completed replacements committed (reported in `ForkResume.applied`) and the rest pending (the host re-runs resume). `sync_dist` commits the deployable packages in one transaction before returning `not_deployed`. `list_with` and `check` never write.

## Risks

- Hosts that relied on `PackageManager.check` must switch to the free function (BREAKING, README).
- Scripts that relied on a missing `--catalog` path being ignored now fail (BREAKING).
