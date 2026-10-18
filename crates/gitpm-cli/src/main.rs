//! `gitpm` command-line client: argument parsing, environment defaults, printing, TTY
//! prompts and command hints per error kind. Business logic lives in the gitpm library.
use anyhow::{Result, bail, ensure};
use gitpm::{
    DistSync, Error as PmError, ListOptions, Manager, Options, ReplacementPlan, auth, catalog,
    check, identity, scaffold,
};
use gitpm_cli::{catalog as catalog_text, check as check_text};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

/// What the command line named, for the hint lines of a failure.
#[derive(Default)]
struct Invocation {
    command: String,
    id: String,
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
            if let PmError::LocalChanges { items, .. } = pm_error {
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
        remote_catalog,
        into,
        multi,
    } = invocation;
    Some(match error {
        PmError::AlreadyInstalled { id, .. } => format!(
            "if {id} was forgotten, `gitpm restore {id}` records it again and `gitpm uninstall {id}` removes its clone"
        ),
        PmError::HasDependents {
            dependents,
            replacing: true,
            ..
        } => format!(
            "switch back first: gitpm replace {} --back",
            dependents.first()?
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
            if *remote_catalog && matches!(command.as_str(), "catalog-update" | "register") =>
        {
            format!("{command} writes a local catalog folder; upload its files yourself")
        }
        PmError::NotDeployed { sync, .. } => {
            let first = &sync.skipped.first()?.id;
            format!(
                "`gitpm uninstall {first}` drops a package whose clone is gone; install it again to deploy it"
            )
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
/// Show which consumers a replacement would leave unsatisfied and decide: no conflict
/// → go; TTY → ask [y/N] (no: the command is cancelled); no TTY → only with --force (the
/// warning is still printed).
fn confirm_replacement(plan: &ReplacementPlan, force: bool) -> Result<bool> {
    if plan.incompatible.is_empty() {
        return Ok(false);
    }
    warn_incompatible(plan);
    if force {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        // Unconfirmed: the library refuses with its typed incompatible_replacement error.
        eprintln!("No terminal to confirm; use --force to replace anyway.");
        return Ok(false);
    }
    ensure!(
        ask("Replace anyway?")?,
        "replacement cancelled; nothing was changed"
    );
    Ok(true)
}
/// A pending replacement of `fork --replace`: warn and ask on a TTY. Unlike
/// `confirm_replacement`, a refusal does not cancel the command (the fork is already
/// released and installed): the replacement stays pending.
fn confirm_pending(plan: &ReplacementPlan) -> Result<bool> {
    warn_incompatible(plan);
    if !io::stdin().is_terminal() {
        eprintln!("No terminal to confirm; use --force to replace anyway.");
        return Ok(false);
    }
    let yes = ask("Replace anyway?")?;
    if !yes {
        eprintln!("{} is not replaced.", plan.original);
    }
    Ok(yes)
}
fn warn_incompatible(plan: &ReplacementPlan) {
    eprintln!(
        "Warning: {} provides {} {}, which these consumers do not accept:",
        plan.provider, plan.original, plan.version
    );
    for consumer in &plan.incompatible {
        eprintln!(
            "  - {} needs {} {}",
            consumer.id, plan.original, consumer.requirement
        );
    }
    eprintln!("The replacement applies to every consumer in this workspace.");
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
/// Ask on a TTY before discarding local work; anything but y/yes keeps everything.
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
/// `installed ID@VERSION from SOURCE`, or `ID already installed (VERSION)` (with `; run
/// update` when the source has another revision, naming the requested ref or channel).
fn print_installed(result: &gitpm::InstallResult, requested: Option<&str>) {
    warn_skipped(&result.skipped);
    warn(&result.warnings);
    if result.already_installed {
        let hint = match (&result.available, requested) {
            (None, _) => String::new(),
            (Some(_), None) => "; run update".to_owned(),
            (Some(_), Some(flags)) => {
                format!("; requested {flags} — run update {} {flags}", result.id)
            }
        };
        println!(
            "{} already installed ({}){hint}",
            result.id, result.current.version
        );
    } else {
        println!(
            "installed {}@{} from {}",
            result.id, result.current.version, result.source
        );
    }
    warn_dist(&result.dist);
}
/// `replace ORIGINAL --back`: which replacement was dropped and who uses the original now.
fn print_switch_back(plan: &ReplacementPlan) {
    let installed = plan.installed_version.as_deref().unwrap_or(&plan.version);
    let consumers = if plan.consumers.is_empty() {
        "no installed consumers".to_owned()
    } else {
        format!("for: {}", plan.consumers.join(", "))
    };
    // switch_back refuses when nothing is replaced, so `current` names the fork; the fork
    // is not touched: say what is left of it.
    let fork = plan.current.as_deref().unwrap_or_default();
    let remains = if plan.fork_enabled {
        format!("; {fork} stays installed and enabled (disable or uninstall it if not wanted)")
    } else {
        format!("; {fork} stays installed (disabled)")
    };
    println!(
        "{} switched back from {fork} ({} {installed} {consumers}){remains}",
        plan.original, plan.original
    );
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
    let mut lines: Vec<(&str, String)> = Vec::new();
    for change in &plan.updates {
        let held = plan.kept_back.iter().find(|h| h.id == change.id);
        lines.push((
            &change.id,
            format!(
                "{} {} {} → {}{}",
                change.id,
                if dry_run {
                    "would be updated"
                } else {
                    "updated"
                },
                change.from.version,
                change.to.version,
                held.map(|h| format!(" (latest {} kept back by dependency constraints)", h.latest))
                    .unwrap_or_default()
            ),
        ));
    }
    for held in &plan.kept_back {
        if !plan.updates.iter().any(|c| c.id == held.id) {
            lines.push((
                &held.id,
                format!(
                    "{} kept back at {} (latest {}: dependency constraints)",
                    held.id, held.selected, held.latest
                ),
            ));
        }
    }
    for unchanged in &plan.unchanged {
        lines.push((
            &unchanged.id,
            format!("{} up to date ({})", unchanged.id, unchanged.version),
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
    for path in &sync.unmanaged {
        eprintln!("Warning: {path} belongs to no known package or is not a folder; left as it is");
    }
    for skipped in &sync.skipped {
        eprintln!(
            "Warning: {}: dist not deployed ({}); run `gitpm sync-dist`",
            skipped.id, skipped.reason
        );
    }
}
/// `ID updated A → B` or `ID up to date (A)` (`verb`: updated, rolled back).
fn print_change(change: &gitpm::RevisionChange, verb: &str) {
    warn_skipped(&change.skipped);
    warn(&change.warnings);
    let short = |commit: &str| commit.chars().take(12).collect::<String>();
    if !change.changed {
        println!("{} up to date ({})", change.id, change.to.version);
    } else if change.from.version == change.to.version {
        println!(
            "{} {verb} {} ({} → {})",
            change.id,
            change.to.version,
            short(&change.from.commit),
            short(&change.to.commit)
        );
    } else {
        println!(
            "{} {verb} {} → {}",
            change.id, change.from.version, change.to.version
        );
    }
    warn_dist(&change.dist);
}
/// What a fork created or added, and the rewritten dependencies.
fn print_package_fork(fork: &gitpm::PackageFork) {
    let root = fork.packages.iter().any(|p| p.path.is_empty());
    if fork.created {
        println!("Fork created: {}", fork.path.display());
        println!(
            "Git history and local edits are kept; the original repository is the upstream remote."
        );
        if !root {
            println!("Only the forked packages are left in the tree and in gitpm.json.");
        }
    } else {
        println!("Packages added to: {}", fork.path.display());
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
/// `fork ID... --ns NAMESPACE --into DIR --replace`: per package release, install,
/// replace (one package or several, same output). A replacement the library left
/// pending (incompatible consumers) is confirmed on a TTY and applied with
/// `fork_replace_resume`. The fork is never removed; every package that did not finish
/// says how to finish or undo.
fn fork_packages_replace(
    manager: &Manager,
    ids: &[&str],
    destination: &Path,
    namespace: &str,
    version: Option<&str>,
    force: bool,
) -> Result<()> {
    let outcome = manager.fork_packages_replace(ids, destination, namespace, version, force)?;
    print_package_fork(&outcome.fork);
    let mut confirmed = Vec::new();
    for pending in outcome.pending() {
        if confirm_pending(&pending.plan)? {
            confirmed.push(pending);
        }
    }
    let gitpm::ForkResume {
        applied: resumed,
        failed: mut resume_failure,
    } = manager.fork_replace_resume(&confirmed, true);
    let quoted = scaffold::shell_quote(&outcome.fork.path.to_string_lossy());
    // Warn about dist/ once, from the report of the last transaction.
    let last_dist = resumed
        .last()
        .map(|plan| &plan.dist)
        .or_else(|| {
            let packages = outcome.packages.iter().rev();
            packages
                .clone()
                .find_map(|p| p.replacement.as_ref().map(|plan| &plan.dist))
                .or_else(|| {
                    packages
                        .clone()
                        .find_map(|p| p.installed.as_ref().map(|i| &i.dist))
                })
        })
        .cloned();
    let mut first_error = None;
    for package in outcome.packages {
        let (original, new_id) = (&package.original, &package.id);
        if let Some(tag) = &package.tag {
            println!("Released: {tag}");
        }
        if let Some(installed) = &package.installed {
            warn_skipped(&installed.skipped);
            warn(&installed.warnings);
            println!("Installed: {new_id} {}", installed.current.version);
        }
        if let Some(plan) = resumed
            .iter()
            .find(|plan| plan.original == *original && plan.provider == *new_id)
        {
            print_replacement(plan);
            continue;
        }
        // A confirmed replacement that failed on resume reports that failure.
        let resumed_error = resume_failure
            .take_if(|f| f.original == *original && f.fork == *new_id)
            .map(|f| f.error);
        let error = match (package.failed, resumed_error.or(package.error)) {
            (None, _) => {
                if let Some(plan) = &package.replacement {
                    print_replacement(plan);
                }
                continue;
            }
            (_, Some(error)) => error,
            (Some(step), None) => {
                PmError::InvalidArgument(format!("fork --replace failed at {}", step.name()))
            }
        };
        let step = package
            .failed
            .map(gitpm::ForkStep::name)
            .unwrap_or("replace");
        eprintln!(
            "{new_id}: the {step} step failed: {error} (kind: {})",
            error.kind()
        );
        match package.failed {
            Some(gitpm::ForkStep::Release) => eprintln!(
                "  Nothing was installed. To finish:\n    gitpm commit {quoted} NEW_VERSION --package {new_id}\n    gitpm install {quoted} {new_id}\n    gitpm replace {original} --with {new_id}"
            ),
            Some(gitpm::ForkStep::Install) => eprintln!(
                "  {new_id} is released but not installed. To finish:\n    gitpm install {quoted} {new_id}\n    gitpm replace {original} --with {new_id}"
            ),
            _ => eprintln!(
                "  {new_id} is installed; {original} is not replaced. To finish:\n    gitpm replace {original} --with {new_id} [--force]\n  To undo: gitpm uninstall {new_id}"
            ),
        }
        first_error.get_or_insert(error);
    }
    if let Some(dist) = &last_dist {
        warn_dist(dist);
    }
    match first_error {
        None => Ok(()),
        Some(error) => {
            if outcome.fork.created {
                eprintln!(
                    "The fork {} is kept. To undo everything: uninstall the installed forks, then delete {quoted}",
                    outcome.fork.path.display()
                );
            } else {
                eprintln!(
                    "The fork {} is kept. To undo everything: uninstall the installed forks, then remove the added package folders from {quoted} (or reset the commit that added them)",
                    outcome.fork.path.display()
                );
            }
            Err(error.into())
        }
    }
}
const COMMANDS: &[&str] = &[
    "init",
    "fork",
    "remote",
    "auth",
    "info",
    "catalog",
    "catalog-update",
    "register",
    "commit",
    "install-catalog",
    "install",
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
        commands: &["install"],
        text: "  install GIT_SOURCE [ID] [--ref REF]  install the latest Git release (tag); --ref picks a
                                   tag, branch or commit; --channel dev follows the default
                                   branch. ID (or --package ID) picks a package of a
                                   repository with gitpm.json (needed when it has several)",
    },
    Help {
        commands: &["install-catalog"],
        text: "  install-catalog ID [--version V]  install an exact catalog release (latest if omitted)",
    },
    Help {
        commands: &["list"],
        text: "  list [--provides SERVICE]        installed state (JSON); --provides keeps only packages
                                   whose plugin.json provides SERVICE",
    },
    Help {
        commands: &["check"],
        text: "  check                            compare installed versions with the catalog",
    },
    Help {
        commands: &["update"],
        text: "  update ID [--ref REF]             update to the latest Git release; --channel dev follows
                                   origin HEAD",
    },
    Help {
        commands: &["update"],
        text: "  update all [--dry-run]           jointly upgrade compatible releases; preview with --dry-run",
    },
    Help {
        commands: &["rollback"],
        text: "  rollback ID                      swap current and previous version",
    },
    Help {
        commands: &["enable", "disable"],
        text: "  enable ID | disable ID [--force]  deploy / remove dist/ID; disable refuses while other
                                   installed packages depend on ID unless --force",
    },
    Help {
        commands: &["forget", "restore"],
        text: "  forget ID [--force] | restore ID  unregister (keeps the clone, removes dist/ID) /
                                   re-register a retained clone",
    },
    Help {
        commands: &["uninstall"],
        text: "  uninstall ID [--force]            remove state, repository and dist; asks on a TTY
                                   before losing local work and refuses while others
                                   depend on ID; --force skips both",
    },
    Help {
        commands: &["fork"],
        text: "  fork ID [ID...] --ns NS --into DIR  fork installed packages of one repository as
                                   NS/<same name> into the fork repository DIR: a new
                                   folder becomes a copy of the source (history, local
                                   edits; unforked sibling packages removed), a repository
                                   with gitpm.json gets the packages added; anything else
                                   is refused. Dependencies between forks are rewritten
    [--replace [--version V] [--force]]  then release each fork (tag NS/NAME@V, V defaults
                                   to the forked version), install it and replace ID with
                                   it for every consumer",
    },
    Help {
        commands: &["replace"],
        text: "  replace ORIGINAL --with FORK [--force]  make FORK provide ORIGINAL for every consumer;
                                   removes dist/ORIGINAL (FORK keeps its own dist/)",
    },
    Help {
        commands: &["replace"],
        text: "  replace ORIGINAL --back [--force]  switch every consumer back to the installed ORIGINAL
                                   and deploy dist/ORIGINAL again
                                   (incompatible consumers: asks on a TTY, else --force)",
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
        text: "  init NAMESPACE/NAME --into DIR   create the package with release 0.1.0:
                                   a new DIR becomes a single-package repository; a
                                   repository DIR with gitpm.json (or without one and no
                                   root package) gets the folder PREFIX/NAME
    [--multi] [--no-release]       --multi: a new (or empty) DIR becomes a multi-package
                                   repository (gitpm.json, packages/NAME); --no-release:
                                   commit only, no tag (release later with commit)",
    },
    Help {
        commands: &["commit"],
        text: "  commit REPO VERSION              stage ALL changes, commit, create a release tag;
    [--package ID]                 --package picks the package of a multi-package repo",
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
        commands: &["info"],
        text: "  info                             show the catalog's directories and package versions",
    },
    Help {
        commands: &["catalog"],
        text: "  catalog                          show directory links at this catalog level (JSON)",
    },
    Help {
        commands: &["catalog-update"],
        text: "  catalog-update [--sources FILE]   generate/refresh a local catalog from the Git release
                                   tags of the repositories listed in FILE",
    },
    Help {
        commands: &["register"],
        text: "  register GIT_SOURCE --ref REF     add one committed version to a local catalog
    [--package ID]                 package of a multi-package repository
    [--build-url HTTPS_URL --build-sha256 SHA256]  optional build metadata",
    },
    Help {
        commands: &[],
        text: "
Settings (flag, then environment, then default; there is no config file):
  --root DIR      GITPM_ROOT       workspace directory; default ./gitpm-workspace
  --catalog LOC   GITPM_CATALOG    catalog folder, catalog.json or URL; no default
                                   (needed by catalog commands, install-catalog, check)
  --sources FILE  GITPM_SOURCES    Git sources list for catalog-update; default ./sources.txt

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
        .filter(|part| part.commands.contains(&command))
        .map(|part| part.text)
        .collect();
    (!blocks.is_empty()).then(|| blocks.join("\n"))
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
    let channel = option(&mut args, "--channel")?;
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
    let ns = option(&mut args, "--ns")?;
    let provides = option(&mut args, "--provides")?;
    let multi = flag(&mut args, "--multi")?;
    let no_release = flag(&mut args, "--no-release")?;
    let replace = flag(&mut args, "--replace")?;
    let back = flag(&mut args, "--back")?;
    let force = flag(&mut args, "--force")?;
    let dry_run = flag(&mut args, "--dry-run")?;
    ensure!(!args.is_empty(), "command required; use --help");
    if let Some(unknown) = args.iter().find(|a| a.len() > 1 && a.starts_with('-')) {
        bail!("unknown option: {unknown}; use --help");
    }
    let command = args[0].as_str();
    ensure!(
        COMMANDS.contains(&command),
        "unknown command: {command}; use --help"
    );
    ensure!(
        sources.is_none() || command == "catalog-update",
        "--sources requires catalog-update"
    );
    ensure!(
        channel.is_none() || matches!(command, "install" | "update"),
        "--channel requires install/update"
    );
    ensure!(
        channel
            .as_deref()
            .is_none_or(|c| c == "dev" || c == "stable"),
        "channel must be stable or dev"
    );
    // `--channel dev --ref REF` is passed on: the library refuses the combination.
    let dev = channel.as_deref() == Some("dev");
    ensure!(
        build.is_none() || command == "register",
        "build options require register"
    );
    ensure!(
        reference.is_none() || matches!(command, "install" | "update" | "register"),
        "--ref is not valid for {command}"
    );
    ensure!(
        version.is_none() || command == "install-catalog" || (command == "fork" && replace),
        "--version is not valid for {command}"
    );
    ensure!(
        !force
            || matches!(command, "uninstall" | "disable" | "forget" | "replace")
            || (command == "fork" && replace),
        "--force is only valid with uninstall/disable/forget/replace/fork --replace"
    );
    ensure!(
        package.is_none() || matches!(command, "install" | "commit" | "register"),
        "--package requires install/commit/register"
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
        (!replace && ns.is_none()) || command == "fork",
        "--ns/--replace require fork"
    );
    ensure!(
        (with.is_none() && !back) || command == "replace",
        "--with/--back require replace"
    );
    ensure!(
        command != "replace" || with.is_some() != back,
        "replace needs exactly one of --with FORK or --back"
    );
    let expected = match command {
        "info" | "catalog-update" | "list" | "check" | "catalog" | "sync-dist" => 1,
        // fork ID [ID...]: one or more installed IDs.
        "fork" => args.len().max(2),
        "auth" | "commit" | "remote" => 3,
        // install SOURCE [ID]: a positional package ID instead of --package.
        "install" if args.len() == 3 => 3,
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
        remote_catalog: catalog_location
            .as_deref()
            .is_some_and(|l| catalog::is_local(l).is_ok_and(|local| !local)),
        into: into.clone(),
        multi,
    };
    if command == "install" && args.len() == 3 {
        ensure!(
            package.is_none(),
            "give the package ID either as an argument or with --package, not both"
        );
    }
    let package = match command {
        "install" if args.len() == 3 => Some(args[2].clone()),
        _ => package,
    };
    let update_all = command == "update" && id == "all";
    ensure!(
        !dry_run || update_all || command == "sync-dist",
        "--dry-run requires update all or sync-dist"
    );
    // `update all` has no ref or channel parameter: these options do not apply to it.
    ensure!(
        !update_all || (reference.is_none() && !dev),
        "update all supports stable releases only; --ref and --channel dev are not supported"
    );
    // Parse IDs and versions with the library's parsers before opening the workspace: a
    // rejected command creates nothing.
    if command == "init" {
        identity::split(id)?;
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
            | "replace"
            | "fork"
    ) && !update_all
    {
        gitpm::valid_id(id)?;
    }
    if command == "fork" {
        // One form only: the namespace of the new IDs and the fork repository are given
        // explicitly; nothing is derived from an ID or a source.
        let ns = ns.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "fork needs --ns NAMESPACE: the namespace of the new packages (NAMESPACE/<same name>)"
            )
        })?;
        identity::valid_part(ns)?;
        ensure!(
            into.is_some(),
            "fork needs --into DIR: the fork repository (a new folder, or a repository with gitpm.json to add the packages to)"
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
    match command {
        "auth" => {
            ensure!(id == "check", "use: gitpm auth check GIT_URL");
            let options = auth::AuthOptions {
                custom_ssh: std::env::var_os("GIT_SSH_COMMAND").is_some()
                    || std::env::var_os("GIT_SSH").is_some(),
            };
            println!("{}", auth::check(&args[2], &options)?.message);
            return Ok(());
        }
        "info" => {
            print!("{}", catalog_text::info(&require_catalog()?)?);
            return Ok(());
        }
        "catalog-update" => {
            let url = require_catalog()?;
            let sources = sources
                .clone()
                .or_else(|| std::env::var("GITPM_SOURCES").ok())
                .unwrap_or(DEFAULT_SOURCES.into());
            let report = catalog::update_sources(&url, Path::new(&sources), &mut |event| {
                let catalog::Progress::ReadingReleases { source } = event;
                eprintln!("Reading releases: {source}");
            })?;
            for skipped in &report.skipped {
                eprintln!("Warning: {}: {}", skipped.source, skipped.tag.reason);
            }
            println!(
                "Catalog ready: {} release(s) checked from sources.",
                report.releases
            );
            return Ok(());
        }
        "init" => {
            let (namespace, name) = identity::split(id)?;
            let dir = into.as_deref().unwrap_or_default();
            let options = scaffold::InitOptions {
                multi,
                release: !no_release,
            };
            let created = scaffold::init_with(Path::new(dir), namespace, name, options)?;
            let source = scaffold::shell_quote(&created.repo.to_string_lossy());
            let root = scaffold::shell_quote(&root);
            let package = match &created.folder {
                None => String::new(),
                Some(_) => format!(" --package {id}"),
            };
            let version = match &created.tag {
                Some(tag) => format!("Git commit and tag {tag}"),
                None => "Git commit, no release tag yet".to_owned(),
            };
            let install = match &created.tag {
                Some(_) => String::new(),
                None => {
                    format!("  gitpm commit {source} 0.1.0{package}   release the first version\n")
                }
            };
            match &created.folder {
                None => println!(
                    "\nCreated: {}\nID: {id}\nVersion 0.1.0: {version}.\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source}\n\nNew release after editing:\n  gitpm commit {source} 0.1.1\n\nGit uses your configured identity; if there is none, the repository gets a local\nPackage Demo <demo@example.invalid>.",
                    created.repo.display(),
                    created.repo.display()
                ),
                Some(folder) => {
                    let path = created.repo.join(folder);
                    println!(
                        "\nCreated: {}\nID: {id}\nVersion 0.1.0: {version} (only gitpm.json and this folder are committed).\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source} {id}\n\nNew release after editing (stages ALL changes of the repository):\n  gitpm commit {source} 0.1.1{package}",
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
            let report = match check::check(Path::new(&root), catalog_location.as_deref()) {
                // Only a non-empty workspace needs the catalog.
                Err(PmError::InvalidArgument(_)) if catalog_location.is_none() => {
                    return Err(require_catalog().unwrap_err());
                }
                report => report?,
            };
            if report.rows.is_empty() {
                println!("No packages installed in workspace {root}.");
                return Ok(());
            }
            println!(
                "Workspace: {root}\nCatalog: {}\n",
                catalog_location.as_deref().unwrap_or_default()
            );
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
        "catalog" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&catalog::links(&require_catalog()?)?)?
            );
            return Ok(());
        }
        "register" => {
            let url = require_catalog()?;
            let reference = reference
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("register requires --ref TAG_OR_COMMIT"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&catalog::register_package(
                    &url,
                    id,
                    reference,
                    package.as_deref(),
                    build
                )?)?
            );
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
        "install-catalog" => {
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
        },
    )?;
    match command {
        "fork" => {
            let namespace = ns.as_deref().unwrap_or_default();
            let ids: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let destination = Path::new(into.as_deref().unwrap_or_default());
            if replace {
                fork_packages_replace(
                    &manager,
                    &ids,
                    destination,
                    namespace,
                    version.as_deref(),
                    force,
                )?;
            } else {
                let fork = manager.fork_packages(&ids, destination, namespace)?;
                print_package_fork(&fork);
                let quoted = scaffold::shell_quote(&fork.path.to_string_lossy());
                println!(
                    "forkOf records each origin; replaces says the fork can stand in for it (used only\nafter an explicit replace). The original installations are unchanged.\n\nNext steps (or all at once: add --replace):"
                );
                for package in &fork.packages {
                    println!(
                        "  gitpm commit {quoted} NEW_VERSION --package {}   commit local edits, tag {}@NEW_VERSION",
                        package.id, package.id
                    );
                }
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
        }
        "install" => {
            let result =
                manager.install_package(id, package.as_deref(), reference.as_deref(), dev)?;
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
                (None, true) => Some("--channel dev".to_owned()),
                (None, false) => None,
            };
            print_installed(&result, requested.as_deref());
        }
        "install-catalog" => {
            print_installed(&manager.install_from_catalog(id, version.as_deref())?, None);
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
                print_change(
                    &manager.update_channel(id, reference.as_deref(), dev)?,
                    "updated",
                );
            }
        }
        "rollback" => print_change(&manager.rollback(id)?, "rolled back"),
        "enable" => {
            let dist = manager.set_enabled(id, true)?;
            println!("{id} enabled");
            warn_dist(&dist);
        }
        "disable" => {
            let dist = manager.set_enabled_with(id, false, force)?;
            println!("{id} disabled");
            warn_dist(&dist);
        }
        "forget" => {
            let dist = manager.forget_with(id, force)?;
            println!("{id} forgotten (clone kept; restore re-registers it)");
            warn_dist(&dist);
        }
        "replace" => {
            let plan = match &with {
                Some(fork) => manager.replace_plan(id, fork)?,
                None => manager.switch_back_plan(id)?,
            };
            let confirm = confirm_replacement(&plan, force)?;
            let done = match &with {
                Some(fork) => {
                    let done = manager.replace(id, fork, confirm)?;
                    print_replacement(&done);
                    done
                }
                None => {
                    let done = manager.switch_back(id, confirm)?;
                    print_switch_back(&done);
                    done
                }
            };
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
            let dist = if force {
                manager.uninstall_with(id, true, true)?
            } else {
                let plan = manager.uninstall_plan(id)?;
                if !plan.dependents.is_empty() {
                    // Report local work too, so one refusal tells everything.
                    for item in &plan.lost_work {
                        eprintln!("Local work that would be lost: {}", item.describe());
                    }
                }
                if !plan.dependents.is_empty()
                    || plan.lost_work.is_empty()
                    || !io::stdin().is_terminal()
                {
                    // Unconfirmed: the library refuses dependents and, without a TTY,
                    // lists the local work.
                    manager.uninstall(id, false)?
                } else if confirm_lost_work(&plan)? {
                    manager.uninstall(id, true)?
                } else {
                    bail!("uninstall cancelled; nothing was changed");
                }
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
