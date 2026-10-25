//! `gitpm` command-line client: argument parsing, environment defaults, printing, TTY
//! prompts and command hints per error kind. Business logic lives in the gitpm library.
use anyhow::{Result, bail, ensure};
use gitpm::{
    DistSync, Error as PmError, ListOptions, Manager, Options, ReplacementPlan, auth, catalog,
    check, scaffold,
};
use gitpm_cli::{catalog as catalog_text, check as check_text};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

/// What the command line named, for the hint lines of a failure.
#[derive(Default)]
struct Invocation {
    command: String,
    id: String,
    with: Option<String>,
    /// The catalog location is an HTTP(S) URL.
    remote_catalog: bool,
    /// `--into DIR` and `--multi` of `init`.
    into: Option<String>,
    multi: bool,
}

fn main() {
    let mut invocation = Invocation::default();
    if let Err(error) = run(&mut invocation) {
        eprintln!("Error: {error:#}");
        if let Some(pm_error) = error.downcast_ref::<PmError>() {
            if let PmError::LocalChanges { items, .. }
            | PmError::HasDependents {
                lost_work: items, ..
            } = pm_error
                && !matches!(invocation.command.as_str(), "update" | "rollback")
            {
                for item in items {
                    eprintln!("  - {}", item.describe());
                }
            }
            // Same stable kind the library API and FFI report.
            eprintln!("Error kind: {}", pm_error.kind());
            if let Some(hint) = hint(&invocation, pm_error) {
                eprintln!("Hint: {hint}");
            }
        }
        std::process::exit(1);
    }
}
/// The gitpm command that helps after a library error, chosen from the command line,
/// the error kind and its structured fields (never from the message).
fn hint(invocation: &Invocation, error: &PmError) -> Option<String> {
    let Invocation {
        command,
        id,
        with,
        remote_catalog,
        into,
        multi,
    } = invocation;
    Some(match error {
        PmError::AlreadyInstalled { id, .. } => format!(
            "if {id} was forgotten, `gitpm restore {id}` records it again and `gitpm uninstall {id}` removes its clone"
        ),
        PmError::HasDependents { id, .. } => {
            format!("`gitpm {command} {id} --force` does it anyway")
        }
        PmError::LocalChanges { id, .. } if command == "uninstall" => {
            format!("`gitpm uninstall {id} --force` deletes it anyway")
        }
        // restore needs the clone; uninstall drops what is left of a package without one.
        PmError::RepositoryNotFound { id } if command == "restore" => format!(
            "`gitpm uninstall {id}` drops a record whose clone is gone; install it again afterwards"
        ),
        // Only where a multi-package repository solves the refusal (the library decides).
        PmError::InvalidArgument(_)
            if command == "init"
                && !multi
                && into
                    .as_deref()
                    .is_some_and(|dir| scaffold::suggests_multi(Path::new(dir))) =>
        {
            format!(
                "`gitpm init {id} --into DIR --multi` makes a new or empty DIR a multi-package repository"
            )
        }
        PmError::InvalidArgument(_)
            if *remote_catalog
                && matches!(command.as_str(), "catalog update" | "catalog register") =>
        {
            format!("`gitpm {command}` writes a local catalog folder; upload its files yourself")
        }
        // The library refused to enable over an inactive dependency.
        PmError::DependencyConflict(_) if command == "enable" => {
            format!("`gitpm enable {id} --force` enables it anyway")
        }
        PmError::DependencyConflict(_) if command == "replace" => {
            let provider = with.as_deref()?;
            format!(
                "`gitpm replace-check {id} --with {provider}` shows dependency issues; `gitpm replace-resolve {id} --with {provider}` installs missing dependencies when resolvable; fix incompatible or disabled installed dependencies first"
            )
        }
        PmError::NotDeployed { sync, .. } => {
            let skipped = sync.skipped.first()?;
            if skipped.clone_missing {
                let id = &skipped.id;
                format!(
                    "`gitpm uninstall {id}` or `gitpm forget {id}` drops a package whose clone is gone; install it again to deploy it"
                )
            } else {
                let path = format!("dist/{}", skipped.id);
                if skipped.reason != format!("{path} is not a folder") {
                    return None;
                }
                format!("remove or rename `{path}` and run `gitpm sync-dist` again")
            }
        }
        _ => return None,
    })
}
fn flag(args: &mut Vec<String>, key: &str) -> Result<bool> {
    ensure!(
        args.iter().filter(|a| *a == key).count() <= 1,
        "{key} given more than once"
    );
    if let Some(index) = args.iter().position(|a| a == key) {
        args.remove(index);
        Ok(true)
    } else {
        Ok(false)
    }
}
/// Ask `[y/N]` on stderr (stdout carries only result lines); EOF counts as no.
fn ask(question: &str) -> Result<bool> {
    eprint!("{question} [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer)? == 0 {
        eprintln!();
        return Ok(false);
    }
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}
fn option(args: &mut Vec<String>, key: &str) -> Result<Option<String>> {
    ensure!(
        args.iter().filter(|a| *a == key).count() <= 1,
        "{key} given more than once"
    );
    if let Some(index) = args.iter().position(|a| a == key) {
        ensure!(index + 1 < args.len(), "{key} requires a value");
        let value = args.remove(index + 1);
        args.remove(index);
        Ok(Some(value))
    } else {
        Ok(None)
    }
}
/// Ask on a TTY before discarding local work; anything but f46bc786-e906-5e84-92ff-6ac4dac44867 keeps everything.
fn confirm_lost_work(plan: &gitpm::UninstallPlan) -> Result<bool> {
    eprintln!("Uninstall {} would delete:", plan.id);
    for path in &plan.deletes {
        eprintln!("  {path}");
    }
    if plan.registered {
        eprintln!("  state.json entry {}", plan.id);
    }
    eprintln!("Local work that would be lost:");
    for item in &plan.lost_work {
        eprintln!("  - {}", item.describe());
    }
    ask("Delete anyway?")
}
/// Report the former provider after removing a replacement binding.
fn print_return_to_original(plan: &ReplacementPlan) {
    let fork = plan.current.as_deref().unwrap_or_default();
    let consumers = if plan.consumers.is_empty() {
        "no installed consumers".to_owned()
    } else {
        format!("for: {}", plan.consumers.join(", "))
    };
    println!(
        "{} switched back from {fork} ({} {} {consumers}); {fork} stays installed and enabled (disable or uninstall it if not wanted)",
        plan.original, plan.original, plan.version
    );
}
/// `ORIGINAL replaced by PROVIDER INSTALLED_VERSION for: consumers`; the version
/// consumers see (a fork's declared `replaces.version`) is added when it differs.
fn print_replacement(plan: &ReplacementPlan) {
    let installed = plan.installed_version.as_deref().unwrap_or(&plan.version);
    println!(
        "{} replaced by {} {installed}{} for: {}",
        plan.original,
        plan.provider,
        if installed == plan.version {
            String::new()
        } else {
            format!(" (as {} {})", plan.original, plan.version)
        },
        if plan.consumers.is_empty() {
            "no consumers yet".to_owned()
        } else {
            plan.consumers.join(", ")
        }
    );
}
/// Library warnings (ignored metadata keys) on stderr.
fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}
/// Broken release tags the library passed over.
fn warn_skipped(skipped: &[gitpm::releases::SkippedTag]) {
    for skipped in skipped {
        eprintln!(
            "Warning: skipped release tag {}: {}",
            skipped.name, skipped.reason
        );
    }
}
/// `installed ID@VERSION from SOURCE`, or for a download `downloaded ID@VERSION from
/// SOURCE (disabled)`.
fn print_fetched(id: &str, version: &str, source: &str, enabled: bool) {
    if enabled {
        println!("installed {id}@{version} from {source}");
    } else {
        println!("downloaded {id}@{version} from {source} (disabled)");
    }
}
/// `installed ID@VERSION from SOURCE` (one line per pulled dependency follows), or `ID
/// already installed (VERSION)` (with `; run
/// update` when the source has another revision, naming the requested `--ref`/`--dev`).
fn print_installed(result: &gitpm::InstallResult, requested: Option<&str>) {
    warn_skipped(&result.skipped);
    warn(&result.warnings);
    if result.already_installed {
        let moves = result.available.is_some() || result.requested_mode.is_some();
        let hint = match (moves, requested) {
            (false, _) => String::new(),
            (true, None) => "; run update".to_owned(),
            (true, Some(flags)) => {
                format!("; requested {flags} — run update {} {flags}", result.id)
            }
        };
        println!(
            "{} already installed ({}{}){hint}",
            result.id,
            result.current.version,
            mode_note(&result.mode)
        );
    } else {
        print_fetched(
            &result.id,
            &result.current.version,
            &result.source,
            result.enabled,
        );
        for dependency in &result.dependencies {
            warn(&dependency.warnings);
            print_fetched(
                &dependency.id,
                &dependency.current.version,
                &dependency.source,
                dependency.enabled,
            );
        }
    }
    warn_dist(&result.dist);
}
/// `update all`: one line per installed package (updated, up to date or kept back) from
/// the plan, then the new dependencies and the totals.
fn print_update_all(plan: &gitpm::UpdateAllPlan, dry_run: bool) {
    warn_skipped(&plan.skipped);
    for change in &plan.updates {
        warn(&change.warnings);
    }
    for install in &plan.installs {
        warn(&install.warnings);
    }
    for unchanged in &plan.unchanged {
        warn(&unchanged.warnings);
    }
    let mut lines: Vec<(&str, String)> = Vec::new();
    for change in &plan.updates {
        let held = plan.kept_back.iter().find(|h| h.id == change.id);
        lines.push((
            &change.id,
            format!(
                "{}{}",
                change_line(
                    change,
                    if dry_run {
                        "would be updated"
                    } else {
                        "updated"
                    }
                ),
                held.map(|h| format!(" (latest {} kept back: {})", h.latest, h.reason))
                    .unwrap_or_default()
            ),
        ));
    }
    for held in &plan.kept_back {
        if !plan.updates.iter().any(|c| c.id == held.id) {
            lines.push((
                &held.id,
                format!(
                    "{} kept back at {} (latest {}: {})",
                    held.id, held.selected, held.latest, held.reason
                ),
            ));
        }
    }
    for unchanged in &plan.unchanged {
        lines.push((
            &unchanged.id,
            unchanged_line(&unchanged.id, &unchanged.version, unchanged.reason.as_ref()),
        ));
    }
    lines.sort();
    for (_, line) in lines {
        println!("{line}");
    }
    for install in &plan.installs {
        println!(
            "{} {} {} (new dependency)",
            install.id,
            if dry_run {
                "would be installed"
            } else {
                "installed"
            },
            install.current.version
        );
    }
    println!(
        "{} updates, {} installs, {} kept back.{}",
        plan.updates.len(),
        plan.installs.len(),
        plan.kept_back.len(),
        if dry_run { " Nothing was changed." } else { "" }
    );
    warn_dist(&plan.dist);
}
/// What sync-dist did (or would do); problems go to stderr.
fn print_dist_sync(sync: &DistSync, dry_run: bool) {
    let (deploy, remove) = if dry_run {
        ("would be deployed to", "would be removed from")
    } else {
        ("deployed to", "removed from")
    };
    for id in &sync.deployed {
        println!("{id} {deploy} dist/{id}");
    }
    for id in &sync.removed {
        println!("{id} {remove} dist/{id}");
    }
    warn_dist(sync);
    if sync.is_empty() && sync.skipped.is_empty() {
        println!("dist/ already holds exactly the active packages.");
    } else if dry_run {
        println!("Nothing was changed (--dry-run).");
    }
}
/// The dist report of a result: folders under dist/ gitpm leaves alone, and active
/// packages left without their deployed folder.
fn warn_dist(sync: &DistSync) {
    warn(&sync.warnings);
    for path in &sync.unmanaged {
        eprintln!("Warning: {path} belongs to no known package or is not a folder; left as it is");
    }
    for skipped in &sync.skipped {
        if skipped.clone_missing {
            eprintln!(
                "Warning: {}: dist not deployed ({}); uninstall or forget {} to drop its record",
                skipped.id, skipped.reason, skipped.id
            );
        } else {
            eprintln!(
                "Warning: {}: dist not deployed ({})",
                skipped.id, skipped.reason
            );
        }
    }
}
/// `ID VERB A → B` (or `ID VERB V (COMMIT → COMMIT)` when the version stays).
fn change_line(change: &gitpm::RevisionChange, verb: &str) -> String {
    let short = |commit: &str| commit.chars().take(12).collect::<String>();
    if change.from.version == change.to.version {
        format!(
            "{} {verb} {} ({} → {})",
            change.id,
            change.to.version,
            short(&change.from.commit),
            short(&change.to.commit)
        )
    } else {
        format!(
            "{} {verb} {} → {}{}",
            change.id,
            change.from.version,
            change.to.version,
            if change.major { " (major)" } else { "" }
        )
    }
}
/// `ID up to date (V)`, or why the package stayed: `ID pinned at V (REF)`, `ID kept at V
/// (history of BRANCH diverged)`, `ID skipped at V (dependency DEP is not installed)`.
fn unchanged_line(id: &str, version: &str, reason: Option<&gitpm::UnchangedReason>) -> String {
    match reason {
        None => format!("{id} up to date ({version})"),
        Some(reason @ gitpm::UnchangedReason::DevLinked { .. }) => format!("{id}: {reason}"),
        Some(gitpm::UnchangedReason::Pinned { reference }) => {
            format!("{id} pinned at {version} ({reference})")
        }
        Some(reason @ gitpm::UnchangedReason::Diverged { .. }) => {
            format!("{id} kept at {version} ({reason})")
        }
        Some(reason @ gitpm::UnchangedReason::BranchGone { .. }) => {
            format!("{id} kept at {version} ({reason})")
        }
        Some(reason @ gitpm::UnchangedReason::BrokenDependency { .. }) => {
            format!("{id} skipped at {version} ({reason})")
        }
        Some(
            reason @ (gitpm::UnchangedReason::NotInCatalog { .. }
            | gitpm::UnchangedReason::CatalogUnavailable { .. }),
        ) => {
            format!("{id} kept at {version} ({reason})")
        }
    }
}
/// The line of a change of mode alone (the revision stays): `ID pinned at V (REF)`,
/// `ID follows branch B at V`, `ID follows releases at V`.
fn mode_line(id: &str, version: &str, mode: &gitpm::Mode) -> String {
    match mode {
        gitpm::Mode::Pinned(reference) => format!("{id} pinned at {version} ({reference})"),
        gitpm::Mode::Branch(branch) => format!("{id} follows branch {branch} at {version}"),
        gitpm::Mode::Release => format!("{id} follows releases at {version}"),
        gitpm::Mode::Link(_) => format!("{id} dev-linked at {version}"),
    }
}
/// `, pinned at REF` / `, following branch B` for the already-installed line.
fn mode_note(mode: &gitpm::Mode) -> String {
    match mode {
        gitpm::Mode::Release => String::new(),
        gitpm::Mode::Link(_) => ", dev-linked".into(),
        gitpm::Mode::Pinned(reference) => format!(", pinned at {reference}"),
        gitpm::Mode::Branch(branch) => format!(", following branch {branch}"),
    }
}
/// `ID updated A → B`, `ID up to date (A)` or `ID pinned at A (REF)` (`verb`: updated,
/// rolled back).
fn print_change(change: &gitpm::RevisionChange, verb: &str) {
    warn_skipped(&change.skipped);
    warn(&change.warnings);
    if let Some(location) = &change.detached {
        println!(
            "{} detached from catalog {location}; it now follows its origin",
            change.id
        );
    }
    if change.changed && change.from.commit == change.to.commit && change.detached.is_some() {
        println!("{} stays at {}", change.id, change.to.version);
    } else if change.changed && change.from.commit == change.to.commit {
        println!(
            "{}",
            mode_line(&change.id, &change.to.version, &change.mode)
        );
    } else if change.changed {
        println!("{}", change_line(change, verb));
    } else {
        println!(
            "{}",
            unchanged_line(
                &change.id,
                &change.to.version,
                change.unchanged_reason.as_ref()
            )
        );
    }
    warn_dist(&change.dist);
}
/// The cloned fork and its rewritten dependencies.
fn print_package_fork(fork: &gitpm::PackageFork) {
    let root = fork.packages.iter().any(|p| p.path.is_empty());
    println!("Fork created: {}", fork.path.display());
    println!(
        "Git history is cloned from the recorded source; the original repository is the upstream remote."
    );
    if !root {
        println!(
            "All repository packages are forked; the complete tree and gitpm.json are preserved."
        );
    }
    for package in &fork.packages {
        println!(
            "{} forked from {} ({})",
            package.id,
            package.original,
            if package.path.is_empty() {
                "repository root"
            } else {
                &package.path
            }
        );
    }
    for rewrite in &fork.rewrites {
        match &rewrite.new_requirement {
            None => println!(
                "Dependency: {} now depends on {} {} (was {})",
                rewrite.package, rewrite.to, rewrite.requirement, rewrite.from
            ),
            Some(new) => println!(
                "Dependency: {} now depends on {} {new} (was {} {}, which the fork's version does not satisfy)",
                rewrite.package, rewrite.to, rewrite.from, rewrite.requirement
            ),
        }
    }
}
const COMMANDS: &[&str] = &[
    "init",
    "fork",
    "remote",
    "auth",
    "catalog show",
    "catalog",
    "catalog links",
    "catalog update",
    "catalog register",
    "commit",
    "install-catalog",
    "install",
    "link",
    "download-catalog",
    "download",
    "list",
    "check",
    "update",
    "rollback",
    "enable",
    "disable",
    "forget",
    "restore",
    "uninstall",
    "replace",
    "replace-check",
    "replace-resolve",
    "sync-dist",
];
const DEFAULT_ROOT: &str = "gitpm-workspace";
const DEFAULT_SOURCES: &str = "sources.txt";
/// The usage text in parts: free text, or the block of the commands it documents.
/// `gitpm --help` prints all parts; `gitpm COMMAND --help` only the blocks of COMMAND.
struct Help {
    commands: &'static [&'static str],
    text: &'static str,
}
const HELP: &[Help] = &[
    Help {
        commands: &[],
        text: "gitpm [--root DIR] [--catalog DIR_OR_JSON_OR_URL] COMMAND

Workspace (installed packages):",
    },
    Help {
        commands: &["link"],
        text: "  link LOCAL_PACKAGE_PATH         live-link the manifest-selected output; relink refreshes metadata",
    },
    Help {
        commands: &["install"],
        text: "  install GIT_SOURCE [ID] [--ref REF | --dev]  install the latest Git release (tag);
                                   --ref TAG|COMMIT pins it, --ref BRANCH follows that
                                   branch of the origin, --dev follows its default branch
                                   (the mode is recorded and update follows it). ID (or
                                   --package ID) picks a package of a repository with
                                   gitpm.json (needed when it has several). Refused while
                                   a dependency is only provided by a disabled package;
    [--force]                      --force enables ID anyway (dependencies as usual).
                                   Over a download from the same source it enables ID",
    },
    Help {
        commands: &["install-catalog"],
        text: "  install-catalog ID [--version V] [--force]  install an exact catalog release (latest
                                   if omitted); --force as for install",
    },
    Help {
        commands: &["download"],
        text: "  download GIT_SOURCE [ID] [--package ID] [--ref REF | --dev]  like install, but record the package and
                                   the dependencies it pulls disabled and deploy nothing;
                                   enable ID later (works offline)",
    },
    Help {
        commands: &["download-catalog"],
        text: "  download-catalog ID [--version V]  like install-catalog, recorded disabled",
    },
    Help {
        commands: &["list"],
        text: "  list [--provides SERVICE]        installed state (JSON); --provides keeps only packages
                                   whose plugin.json provides SERVICE",
    },
    Help {
        commands: &["check"],
        text: "  check [--no-catalog]             compare with catalog data, or shallow origin snapshots
                                   --no-catalog forces origin checks for Git packages",
    },
    Help {
        commands: &["update"],
        text: "  update ID [--ref REF | --dev | --release]  follow the recorded mode (latest release,
                                   branch HEAD; a pinned package stays); --ref/--dev switch
                                   as in install, --release returns to releases. A package
                                   installed from a verified catalog moves only to the
                                   versions that catalog lists (hashes checked)
    [--detach]                     drop that protection and update from the origin",
    },
    Help {
        commands: &["update"],
        text: "  update all [--dry-run]           jointly upgrade each package by its recorded mode;
                                   preview with --dry-run",
    },
    Help {
        commands: &["rollback"],
        text: "  rollback ID                      swap current and previous version",
    },
    Help {
        commands: &["enable", "disable"],
        text: "  enable ID | disable ID [--force]  deploy / remove dist/ID; enable refuses while a
                                   dependency of ID is not active (disabled, missing,
                                   replaced away, other version), disable while other
                                   installed packages would break; disabling a fork
                                   drops its replacement; --force does it anyway",
    },
    Help {
        commands: &["forget", "restore"],
        text: "  forget ID [--force] | restore ID  unregister (keeps the clone and the record, removes
                                   dist/ID) / record it again as it was",
    },
    Help {
        commands: &["uninstall"],
        text: "  uninstall ID [--force]            remove state, repository and dist; asks on a TTY
                                   before losing local work and refuses while others
                                   would break; --force confirms what the plan lists
                                   and ignores dependents; a package whose clone is
                                   gone is removed without asking",
    },
    Help {
        commands: &["fork"],
        text: "  fork ID [ID...] --into DIR  fork installed packages with fresh UUIDs
                                   and preserved aliases into repository DIR: a new
                                   folder clones the recorded source with Git history
                                   at installed revisions; local installation edits are
                                   ignored. Every repository package gets a new UUID;
                                   all files and the package layout are retained.
                                   Existing destinations are refused. Dependencies
                                   between packages forked together are rewritten
                                   No release, installation or replacement is performed;
                                   use commit, install and replace explicitly",
    },
    Help {
        commands: &["replace"],
        text: "  replace ORIGINAL --with PROVIDER  apply a consistent global replacement;
                                   use ORIGINAL as PROVIDER to remove the binding;
                                   no confirmation, force or automatic installs",
    },
    Help {
        commands: &["replace-check"],
        text: "  replace-check ORIGINAL --with PROVIDER  report workspace issues and missing installs as JSON;
                                   does not change the workspace",
    },
    Help {
        commands: &["replace-resolve"],
        text: "  replace-resolve ORIGINAL --with PROVIDER  explicitly install missing dependencies
                                   from the catalog without applying the binding",
    },
    Help {
        commands: &["sync-dist"],
        text: "  sync-dist [--dry-run]            make dist/ hold exactly the active packages (installed,
                                   enabled, not replaced); mutating commands
                                   reconcile dist/ automatically",
    },
    Help {
        commands: &[],
        text: "
Authoring:",
    },
    Help {
        commands: &["init"],
        text: "  init NAME --into DIR   create a package (0.1.0 in a new repository):
                                   a new DIR becomes a single-package repository; a
                                   repository DIR with gitpm.json (or without one and no
                                   root package) gets PREFIX/UUID at the repository
                                   version, without creating a release tag
    [--multi] [--no-release]       --multi: a new (or empty) DIR becomes a multi-package
                                   repository (gitpm.json, packages/UUID); --no-release:
                                   commit only, no tag (release later with commit)",
    },
    Help {
        commands: &["commit"],
        text: "  commit REPO VERSION              set ALL package versions, stage ALL changes,
                                   commit and create one vVERSION release tag;
    [--package ID]                 --package picks only the package ID in the result",
    },
    Help {
        commands: &["remote"],
        text: "  remote REPO URL                  add or replace origin (fetch and push)",
    },
    Help {
        commands: &["auth"],
        text: "  auth check GIT_URL               check repository read access without installing",
    },
    Help {
        commands: &[],
        text: "
Catalog (a local folder of static JSON files, or an HTTP(S) URL for reading):",
    },
    Help {
        commands: &["catalog show"],
        text: "  catalog show                     show the catalog's directories and package versions",
    },
    Help {
        commands: &["catalog links"],
        text: "  catalog links                    show directory links at this catalog level (JSON)",
    },
    Help {
        commands: &["catalog update"],
        text: "  catalog update [--sources FILE]   generate/refresh a local catalog from the Git release
                                   tags of the repositories listed in FILE",
    },
    Help {
        commands: &["catalog register"],
        text: "  catalog register GIT_SOURCE --ref REF  add one committed version to a local catalog
    [--package ID]                 package of a multi-package repository
    [--build-url HTTPS_URL --build-sha256 SHA256]  optional build metadata",
    },
    Help {
        commands: &[],
        text: "
Settings (flag, then environment, then default; there is no config file):
  --root DIR      GITPM_ROOT       workspace directory; default ./gitpm-workspace
  --catalog LOC   GITPM_CATALOG    catalog folder, catalog.json or URL; no default
                                   (needed by catalog commands, install-catalog and
                                   download-catalog)
  --sources FILE  GITPM_SOURCES    Git sources list for catalog update; default ./sources.txt

Nothing is built or executed and nothing is pushed or uploaded: gitpm clones Git
repositories and copies their dist/ folder.",
    },
];
/// The whole usage text (`gitpm --help`).
fn usage() -> String {
    HELP.iter()
        .map(|part| part.text)
        .collect::<Vec<_>>()
        .join("\n")
}
/// The usage blocks of one command (`gitpm COMMAND --help`); `None`: not a command.
fn command_usage(command: &str) -> Option<String> {
    let blocks: Vec<&str> = HELP
        .iter()
        .filter(|part| {
            part.commands.contains(&command)
                || (command == "catalog" && part.commands.iter().any(|c| c.starts_with("catalog ")))
        })
        .map(|part| part.text)
        .collect();
    (!blocks.is_empty()).then(|| blocks.join("\n"))
}
/// Collapse a catalog operation to the same command key used by help and validation.
fn catalog_command(args: &mut Vec<String>, help: bool) -> Result<()> {
    if args.first().is_none_or(|arg| arg != "catalog") {
        return Ok(());
    }
    match args.get(1).map(String::as_str) {
        Some("show" | "links" | "update" | "register") => {
            let subcommand = args.remove(1);
            args[0] = format!("catalog {subcommand}");
        }
        Some("--help" | "-h") if help => {}
        None => bail!(
            "catalog needs a subcommand: show, links, update or register; use gitpm catalog --help"
        ),
        Some(other) => bail!("unknown catalog command: {other}; use gitpm catalog --help"),
    }
    Ok(())
}
fn run(invocation: &mut Invocation) -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root = option(&mut args, "--root")?
        .or_else(|| std::env::var("GITPM_ROOT").ok())
        .unwrap_or(DEFAULT_ROOT.into());
    let catalog_location = option(&mut args, "--catalog")?
        .or_else(|| std::env::var("GITPM_CATALOG").ok())
        .filter(|location| !location.is_empty());
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{}", usage());
        return Ok(());
    }
    if matches!(args[0].as_str(), "--version" | "-V") {
        println!("gitpm {}", gitpm_cli::version());
        return Ok(());
    }
    // `gitpm COMMAND --help`: that command's usage only.
    if args[1..].iter().any(|a| a == "--help" || a == "-h") {
        catalog_command(&mut args, true)?;
        let command = args[0].as_str();
        let text = COMMANDS
            .contains(&command)
            .then(|| command_usage(command))
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("unknown command: {command}; use --help"))?;
        println!("{text}");
        return Ok(());
    }
    let reference = option(&mut args, "--ref")?;
    let version = option(&mut args, "--version")?;
    let sources = option(&mut args, "--sources")?;
    let build_url = option(&mut args, "--build-url")?;
    let build_sha256 = option(&mut args, "--build-sha256")?;
    let build = match (build_url, build_sha256) {
        (None, None) => None,
        (Some(url), Some(sha256)) => Some(catalog::Build { url, sha256 }),
        _ => bail!("--build-url and --build-sha256 must be provided together"),
    };
    let with = option(&mut args, "--with")?;
    let package = option(&mut args, "--package")?;
    let into = option(&mut args, "--into")?;
    let provides = option(&mut args, "--provides")?;
    let multi = flag(&mut args, "--multi")?;
    let no_release = flag(&mut args, "--no-release")?;
    let no_catalog = flag(&mut args, "--no-catalog")?;
    let force = flag(&mut args, "--force")?;
    let dry_run = flag(&mut args, "--dry-run")?;
    let dev = flag(&mut args, "--dev")?;
    let release = flag(&mut args, "--release")?;
    let detach = flag(&mut args, "--detach")?;
    catalog_command(&mut args, false)?;
    ensure!(!args.is_empty(), "command required; use --help");
    if let Some(unknown) = args.iter().find(|a| a.len() > 1 && a.starts_with('-')) {
        bail!("unknown option: {unknown}; use --help");
    }
    let command = args[0].as_str();
    ensure!(
        !no_catalog || command == "check",
        "--no-catalog requires check"
    );
    let catalog_location = if no_catalog { None } else { catalog_location };
    ensure!(
        COMMANDS.contains(&command),
        "unknown command: {command}; use --help"
    );
    ensure!(
        sources.is_none() || command == "catalog update",
        "--sources requires catalog update"
    );
    ensure!(
        !dev || matches!(command, "install" | "download" | "update"),
        "--dev requires install/download/update"
    );
    ensure!(!release || command == "update", "--release requires update");
    ensure!(
        !detach || command == "update",
        "--detach requires update ID"
    );
    ensure!(!dev || reference.is_none(), "--dev conflicts with --ref");
    ensure!(
        !release || reference.is_none(),
        "--release conflicts with --ref"
    );
    ensure!(!release || !dev, "--release conflicts with --dev");
    ensure!(
        build.is_none() || command == "catalog register",
        "build options require catalog register"
    );
    ensure!(
        reference.is_none()
            || matches!(
                command,
                "install" | "download" | "update" | "catalog register"
            ),
        "--ref is not valid for {command}"
    );
    ensure!(
        version.is_none() || matches!(command, "install-catalog" | "download-catalog"),
        "--version is not valid for {command}"
    );
    ensure!(
        !force
            || matches!(
                command,
                "uninstall" | "disable" | "enable" | "install" | "install-catalog" | "forget"
            ),
        "--force is only valid with uninstall/disable/enable/install/install-catalog/forget"
    );
    ensure!(
        package.is_none()
            || matches!(
                command,
                "install" | "download" | "commit" | "catalog register"
            ),
        "--package requires install/download/commit/catalog register"
    );
    ensure!(
        into.is_none() || matches!(command, "init" | "fork"),
        "--into requires init/fork"
    );
    ensure!(
        provides.is_none() || command == "list",
        "--provides requires list"
    );
    ensure!(
        (!multi && !no_release) || command == "init",
        "--multi/--no-release require init"
    );
    ensure!(
        with.is_none() || matches!(command, "replace" | "replace-check" | "replace-resolve"),
        "--with requires replace/replace-check/replace-resolve"
    );
    ensure!(
        !matches!(command, "replace" | "replace-check" | "replace-resolve") || with.is_some(),
        "{command} needs --with PROVIDER"
    );
    let expected = match command {
        "catalog show" | "catalog update" | "list" | "check" | "catalog links" | "sync-dist" => 1,
        // fork ID [ID...]: one or more installed IDs.
        "fork" => args.len().max(2),
        "auth" | "commit" | "remote" => 3,
        // install SOURCE [ID]: a positional package ID instead of --package.
        "install" | "download" if args.len() == 3 => 3,
        _ => 2,
    };
    ensure!(
        args.len() == expected,
        "{command}: expected {} argument(s), got {}; use --help",
        expected - 1,
        args.len() - 1
    );
    let id = args.get(1).map(String::as_str).unwrap_or("");
    *invocation = Invocation {
        command: command.to_owned(),
        id: id.to_owned(),
        with: with.clone(),
        remote_catalog: catalog_location
            .as_deref()
            .is_some_and(|l| catalog::is_local(l).is_ok_and(|local| !local)),
        into: into.clone(),
        multi,
    };
    if matches!(command, "install" | "download") && args.len() == 3 {
        ensure!(
            package.is_none(),
            "give the package ID either as an argument or with --package, not both"
        );
    }
    let package = match command {
        "install" | "download" if args.len() == 3 => Some(args[2].clone()),
        _ => package,
    };
    let update_all = command == "update" && id == "all";
    ensure!(
        !dry_run || update_all || command == "sync-dist",
        "--dry-run requires update all or sync-dist"
    );
    // `update all` takes no target: these options do not apply to it.
    ensure!(
        !update_all || (reference.is_none() && !dev && !release),
        "update all takes no --ref, --dev or --release; it follows each package's recorded mode"
    );
    ensure!(
        !update_all || !detach,
        "update all takes no --detach; detach one package with update ID --detach"
    );
    // Parse IDs and versions with the library's parsers before opening the workspace: a
    // rejected command creates nothing.
    if command == "init" {
        scaffold::validate_name(id)?;
        ensure!(
            into.is_some(),
            "{command} needs --into DIR: the repository (a new folder, or a repository with gitpm.json to add the package to)"
        );
    }
    if let Some(package) = &package {
        gitpm::valid_id(package)?;
    }
    if matches!(
        command,
        "update"
            | "rollback"
            | "enable"
            | "disable"
            | "forget"
            | "restore"
            | "uninstall"
            | "install-catalog"
            | "download-catalog"
            | "replace"
            | "replace-check"
            | "replace-resolve"
            | "fork"
    ) && !update_all
    {
        gitpm::valid_id(id)?;
    }
    if command == "fork" {
        ensure!(
            into.is_some(),
            "fork needs --into DIR: the fork repository (a new folder)"
        );
        for other in &args[1..] {
            gitpm::valid_id(other)?;
        }
    }
    if let Some(fork) = &with {
        gitpm::valid_id(fork)?;
    }
    if let Some(version) = &version {
        catalog::version_key(version)
            .map_err(|e| anyhow::anyhow!("invalid --version {version}: {e}"))?;
    }
    // Catalog commands need an explicit location; there is no built-in catalog.
    let require_catalog = || -> Result<String> {
        catalog_location.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "{command} needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG"
            )
        })
    };
    let custom_ssh =
        std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some();
    let ssh_options = Options {
        custom_ssh,
        ..Options::default()
    };
    match command {
        "auth" => {
            ensure!(id == "check", "use: gitpm auth check GIT_URL");
            println!("{}", auth::check(&args[2], &ssh_options)?.message);
            return Ok(());
        }
        "catalog show" => {
            print!("{}", catalog_text::info(&require_catalog()?)?);
            return Ok(());
        }
        "catalog update" => {
            let url = require_catalog()?;
            let sources = sources
                .clone()
                .or_else(|| std::env::var("GITPM_SOURCES").ok())
                .unwrap_or(DEFAULT_SOURCES.into());
            let report = catalog::update_sources_with_options(
                &url,
                Path::new(&sources),
                &mut |event| {
                    let catalog::Progress::ReadingReleases { source } = event;
                    eprintln!("Reading releases: {source}");
                },
                &ssh_options,
            )?;
            for skipped in &report.skipped {
                eprintln!("Warning: {}: {}", skipped.source, skipped.tag.reason);
            }
            warn(&report.warnings);
            println!(
                "Catalog ready: {} release(s) checked from sources.",
                report.releases
            );
            return Ok(());
        }
        "init" => {
            let name = id;
            let dir = into.as_deref().unwrap_or_default();
            let options = scaffold::InitOptions {
                multi,
                release: !no_release,
            };
            let created = scaffold::init_with(Path::new(dir), name, options)?;
            let id = created.id.as_str();
            let source = scaffold::shell_quote(&created.repo.to_string_lossy());
            let root = scaffold::shell_quote(&root);
            let package = match &created.folder {
                None => String::new(),
                Some(_) => format!(" --package {id}"),
            };
            let package_version = &created.version;
            let next_version = if created.new_repository {
                "0.1.1"
            } else {
                "NEW_VERSION"
            };
            let first_version = if created.new_repository {
                "0.1.0"
            } else {
                "NEW_VERSION"
            };
            let version = match &created.tag {
                Some(tag) => format!("Git commit and tag {tag}"),
                None => "Git commit, no release tag yet".to_owned(),
            };
            let install = match &created.tag {
                Some(_) => String::new(),
                None => {
                    format!(
                        "  gitpm commit {source} {first_version}{package}   release all repository packages\n"
                    )
                }
            };
            match &created.folder {
                None => println!(
                    "\nCreated: {}\nID: {id}\nVersion {package_version}: {version}.\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source}\n\nNew release after editing:\n  gitpm commit {source} {next_version}\n\nGit uses your configured identity; if there is none, the repository gets a local\nPackage Demo <demo@example.invalid>.",
                    created.repo.display(),
                    created.repo.display()
                ),
                Some(folder) => {
                    let path = created.repo.join(folder);
                    println!(
                        "\nCreated: {}\nID: {id}\nVersion {package_version}: {version} (only gitpm.json and this folder are committed).\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source} {id}\n\nNew release after editing (stages ALL changes of the repository):\n  gitpm commit {source} {next_version}{package}",
                        path.display(),
                        path.display()
                    );
                }
            }
            return Ok(());
        }
        "list" => {
            // Read-only: never opens, locks or creates the workspace.
            let view = gitpm::list_with(Path::new(&root), &ListOptions { provides })?;
            println!("{}", serde_json::to_string_pretty(&view)?);
            return Ok(());
        }
        "check" => {
            let report = if no_catalog {
                check::check_without_catalog(Path::new(&root), &ssh_options)?
            } else {
                check::check_with_options(
                    Path::new(&root),
                    catalog_location.as_deref(),
                    &ssh_options,
                )?
            };
            if report.rows.is_empty() {
                println!("No packages installed in workspace {root}.");
                return Ok(());
            }
            match catalog_location.as_deref() {
                Some(location) => println!(
                    "Workspace: {root}\nCatalog: {location} ({})\n",
                    if report.catalog_verified {
                        "verified"
                    } else {
                        "unverified: discovery only"
                    }
                ),
                None => println!("Workspace: {root}\nCatalog: none (each package's origin)\n"),
            }
            let color = io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM").as_deref() != Ok("dumb");
            print!("{}", check_text::display_colored(&report.rows, color));
            for (original, fork) in &report.replacements {
                println!("Replacement: {original} → {fork} (for every consumer)");
            }
            for dep in &report.unsatisfied {
                println!(
                    "Unsatisfied dependency: {} → {} {}: {}",
                    dep.package,
                    dep.dependency,
                    dep.requirement,
                    dep.problem.as_deref().unwrap_or("")
                );
            }
            return Ok(());
        }
        "catalog links" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&catalog::links(&require_catalog()?)?)?
            );
            return Ok(());
        }
        "catalog register" => {
            let url = require_catalog()?;
            let reference = reference
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("catalog register requires --ref TAG_OR_COMMIT"))?;
            let report = catalog::register_package_report_with_options(
                &url,
                id,
                reference,
                package.as_deref(),
                build,
                &ssh_options,
            )?;
            warn(&report.warnings);
            println!("{}", serde_json::to_string_pretty(&report.release)?);
            return Ok(());
        }
        "remote" => {
            scaffold::set_remote(Path::new(id), &args[2])?;
            println!("Origin for {id} set to {} (fetch and push).", args[2]);
            return Ok(());
        }
        "commit" => {
            let done = scaffold::commit_package(Path::new(id), &args[2], package.as_deref())?;
            println!(
                "{} committed {} (local commit and tag {})",
                done.package, done.version, done.tag
            );
            return Ok(());
        }
        "install-catalog" | "download-catalog" => {
            require_catalog()?;
        }
        _ => {}
    }
    // The catalog location is passed as given; the library refuses one that does not
    // exist before anything is created.
    let manager = Manager::open_with(
        root,
        Options {
            catalog: catalog_location,
            custom_ssh,
        },
    )?;
    match command {
        "fork" => {
            let ids: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let destination = Path::new(into.as_deref().unwrap_or_default());
            let fork = manager.fork_packages(&ids, destination)?;
            print_package_fork(&fork);
            let quoted = scaffold::shell_quote(&fork.path.to_string_lossy());
            println!(
                "forkOf records each origin; replaces says the fork can stand in for it (used only\nafter an explicit replace). The original installations are unchanged.\n\nNext steps (choose the packages to install and replace):"
            );
            println!(
                "  gitpm commit {quoted} NEW_VERSION   commit local edits, release all packages with tag vNEW_VERSION"
            );
            for package in &fork.packages {
                println!("  gitpm install {quoted} {}", package.id);
            }
            for package in &fork.packages {
                println!("  gitpm replace {} --with {}", package.original, package.id);
            }
            if fork.created {
                println!("Set your own origin with: gitpm remote {quoted} GIT_URL");
            }
        }
        "link" => {
            let result = manager.link(id)?;
            println!("{} dev-linked {}", result.id, result.version);
            print_dist_sync(&result.dist, false);
        }
        "install" | "download" => {
            let target = match (&reference, dev) {
                (Some(reference), _) => gitpm::InstallTarget::Ref(reference.clone()),
                (None, true) => gitpm::InstallTarget::Dev,
                (None, false) => gitpm::InstallTarget::Release,
            };
            let result = if command == "install" {
                manager.install_package_with(id, package.as_deref(), target, force)?
            } else {
                manager.download(id, package.as_deref(), target)?
            };
            let requested = match (&reference, dev) {
                (Some(reference), _) => Some(format!(
                    "--ref {}",
                    if reference
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "@._/+-".contains(c))
                    {
                        reference.clone()
                    } else {
                        scaffold::shell_quote(reference)
                    }
                )),
                (None, true) => Some("--dev".to_owned()),
                (None, false) => None,
            };
            print_installed(&result, requested.as_deref());
        }
        "install-catalog" => {
            print_installed(
                &manager.install_from_catalog_with(id, version.as_deref(), force)?,
                None,
            );
        }
        "download-catalog" => {
            print_installed(
                &manager.download_from_catalog(id, version.as_deref())?,
                None,
            );
        }
        "update" => {
            if update_all {
                let plan = if dry_run {
                    manager.update_all_plan()?
                } else {
                    manager.update_all()?
                };
                print_update_all(&plan, dry_run);
            } else {
                let target = match (reference, dev, release) {
                    (Some(reference), _, _) => gitpm::Target::Ref(reference),
                    (None, true, _) => gitpm::Target::Dev,
                    (None, false, true) => gitpm::Target::Release,
                    (None, false, false) => gitpm::Target::Recorded,
                };
                print_change(
                    &manager.update_with(id, target, gitpm::UpdateOptions { detach })?,
                    "updated",
                );
            }
        }
        "rollback" => print_change(&manager.rollback(id)?, "rolled back"),
        "enable" => {
            let dist = manager.enable_with(id, force)?;
            println!("{id} enabled");
            warn_dist(&dist);
        }
        "disable" => {
            let dist = manager.set_enabled_with(id, false, force)?;
            println!("{id} disabled");
            warn_dist(&dist);
        }
        "forget" => {
            let forgotten = manager.forget_with(id, force)?;
            if forgotten.clone_kept {
                println!("{id} forgotten (clone kept; restore re-registers it)");
            } else {
                println!("{id} forgotten (no clone; record and dist removed)");
            }
            warn_dist(&forgotten.dist);
        }
        "replace" => {
            let done = manager.replace(id, with.as_deref().unwrap())?;
            if done.provider == done.original {
                if done.current.is_some() {
                    print_return_to_original(&done);
                } else {
                    println!("{} already uses original {}", done.original, done.version);
                }
            } else {
                print_replacement(&done);
            }
            warn_dist(&done.dist);
        }
        "replace-check" => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &manager.replace_check(id, with.as_deref().unwrap())?
                )?
            );
        }
        "replace-resolve" => {
            let done = manager.replace_resolve(id, with.as_deref().unwrap())?;
            for installed in &done.installed {
                warn(&installed.warnings);
                print_fetched(
                    &installed.id,
                    &installed.current.version,
                    &installed.source,
                    installed.enabled,
                );
            }
            warn_dist(&done.dist);
        }
        "restore" => {
            let restored = manager.restore(id)?;
            warn(&restored.warnings);
            println!("{id} restored {}", restored.current.version);
            warn_dist(&restored.dist);
        }
        "sync-dist" => {
            if dry_run {
                print_dist_sync(&manager.sync_dist_plan()?, true);
            } else {
                match manager.sync_dist() {
                    Ok(sync) => print_dist_sync(&sync, false),
                    Err(error) => {
                        // What could be synced is committed; say what it was.
                        if let PmError::NotDeployed { sync, .. } = &error {
                            print_dist_sync(sync, false);
                        }
                        return Err(error.into());
                    }
                }
            }
        }
        "uninstall" => {
            let plan = manager.uninstall_plan(id)?;
            // The confirmation covers exactly the items shown (or, forced, planned): the
            // library refuses work that appeared since.
            let dist = if force {
                manager.uninstall(id, &plan.lost_work, true)?
            } else if let Some(refusal) = plan.refusal() {
                // Dependents and the local work, in one refusal.
                return Err(refusal.into());
            } else if plan.lost_work.is_empty() || !io::stdin().is_terminal() {
                // Nothing confirmed: without a TTY the library lists the local work.
                manager.uninstall(id, &[], false)?
            } else if confirm_lost_work(&plan)? {
                manager.uninstall(id, &plan.lost_work, false)?
            } else {
                bail!("uninstall cancelled; nothing was changed");
            };
            println!("{id} uninstalled");
            warn_dist(&dist);
        }
        _ => bail!("unknown command: {command}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_usage_block() {
        for command in COMMANDS {
            let text = command_usage(command).unwrap_or_else(|| panic!("{command}"));
            assert!(usage().contains(&text), "{command}");
            assert!(text.contains(&format!(" {command} ")), "{command}: {text}");
        }
        assert!(command_usage("nope").is_none());
    }
}
