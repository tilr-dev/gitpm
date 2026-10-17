//! `gitpm` command-line client: argument parsing, environment defaults, printing and
//! prompts. Business logic lives in the gitpm library.
use anyhow::{Result, bail, ensure};
use gitpm::{Error as PmError, Manager, Options, auth, catalog, check, identity, scaffold};
use gitpm_cli::{catalog as catalog_text, check as check_text};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        if let Some(pm_error) = error.downcast_ref::<PmError>() {
            if let PmError::LocalChanges { items, .. } = pm_error {
                for item in items {
                    eprintln!("  - {}", item.describe());
                }
            }
            // Same stable kind the library API and FFI report.
            eprintln!("Error kind: {}", pm_error.kind());
        }
        std::process::exit(1);
    }
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
/// Show which consumers a replacement would leave unsatisfied and decide: no conflict
/// → go; TTY → ask [y/N]; no TTY → only with --force (the warning is still printed).
fn confirm_replacement(plan: &gitpm::ReplacementPlan, force: bool) -> Result<bool> {
    if plan.incompatible.is_empty() {
        return Ok(false);
    }
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
    if force {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        // Unconfirmed: the library refuses with its typed incompatible_replacement error.
        eprintln!("No terminal to confirm; use --force to replace anyway.");
        return Ok(false);
    }
    eprint!("Replace anyway? [y/N] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer)? == 0 {
        eprintln!();
        bail!("replacement cancelled; nothing was changed");
    }
    ensure!(
        matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"),
        "replacement cancelled; nothing was changed"
    );
    Ok(true)
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
/// Split the init/create argument `NAMESPACE/NAME` (a plugin ID, not a path). The
/// namespace is mandatory: there is no default and no config file.
fn split_id(value: &str) -> Result<(&str, &str)> {
    let Some((namespace, name)) = value.split_once('/') else {
        bail!("plugin id needs a namespace: use NAMESPACE/NAME, for example acme/{value}");
    };
    identity::qualified(namespace, name)?;
    Ok((namespace, name))
}
/// Ask on a TTY before discarding local work; anything but y/yes keeps everything.
fn confirm_lost_work(plan: &gitpm::UninstallPlan) -> Result<bool> {
    println!("Uninstall {} would delete:", plan.id);
    for path in &plan.deletes {
        println!("  {path}");
    }
    if plan.registered {
        println!("  state.json entry {}", plan.id);
    }
    println!("Local work that would be lost:");
    for item in &plan.lost_work {
        println!("  - {}", item.describe());
    }
    print!("Delete anyway? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer)? == 0 {
        println!();
        return Ok(false);
    }
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}
/// `ORIGINAL → PROVIDER INSTALLED_VERSION for: consumers`; the version consumers see
/// (a fork's declared `replaces.version`) is added when it differs.
fn print_replacement(manager: &Manager, plan: &gitpm::ReplacementPlan) -> Result<()> {
    let installed = manager
        .state()?
        .plugins
        .get(&plan.provider)
        .map(|p| p.current.version.clone())
        .unwrap_or_else(|| plan.version.clone());
    println!(
        "{} → {} {installed}{} for: {}",
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
    Ok(())
}
/// Library warnings (ignored metadata keys) on stderr.
fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}
/// `installed ID@VERSION from SOURCE`.
fn print_installed(result: &gitpm::InstallResult) {
    warn(&result.warnings);
    println!(
        "installed {}@{} from {}",
        result.id, result.current.version, result.source
    );
}
/// `replace ORIGINAL --back`: which replacement was dropped and who uses the original now.
fn print_switch_back(manager: &Manager, plan: &gitpm::ReplacementPlan) -> Result<()> {
    let installed = manager
        .state()?
        .plugins
        .get(&plan.original)
        .map(|p| p.current.version.clone())
        .unwrap_or_else(|| plan.version.clone());
    let consumers = if plan.consumers.is_empty() {
        "no installed consumers".to_owned()
    } else {
        format!("for: {}", plan.consumers.join(", "))
    };
    // switch_back refuses when nothing is replaced, so `current` names the fork.
    let fork = plan.current.as_deref().unwrap_or_default();
    // The fork is not touched: say what is left of it.
    let remains = match manager.state()?.plugins.get(fork) {
        Some(p) if p.enabled => {
            format!("; {fork} stays installed and enabled (disable or uninstall it if not wanted)")
        }
        Some(_) => format!("; {fork} stays installed (disabled)"),
        None => String::new(),
    };
    println!(
        "{}: replacement {fork} removed, original restored ({} {installed} {consumers}){remains}",
        plan.original, plan.original
    );
    Ok(())
}
/// `update all`: one line per installed package (updated, up to date or kept back),
/// then the new dependencies and the totals.
fn print_update_all(before: &gitpm::State, plan: &gitpm::UpdateAllPlan, dry_run: bool) {
    for change in &plan.updates {
        warn(&change.warnings);
    }
    for install in &plan.installs {
        warn(&install.warnings);
    }
    for (id, installed) in &before.plugins {
        let held = plan.kept_back.iter().find(|h| &h.id == id);
        match plan.updates.iter().find(|c| &c.id == id) {
            Some(change) => println!(
                "{id} {} {} → {}{}",
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
            None => match held {
                Some(h) => println!(
                    "{id} kept back at {} (latest {}: dependency constraints)",
                    installed.current.version, h.latest
                ),
                None => println!("{id} up to date ({})", installed.current.version),
            },
        }
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
}
/// What sync-dist did (or would do); problems go to stderr.
fn print_dist_sync(sync: &gitpm::DistSync, dry_run: bool) {
    let (deploy, remove) = if dry_run {
        ("Would deploy", "Would remove")
    } else {
        ("Deployed", "Removed")
    };
    for id in &sync.deployed {
        println!("{deploy}: dist/{id}");
    }
    for id in &sync.removed {
        println!("{remove}: dist/{id}");
    }
    warn_dist(sync);
    if sync.is_empty() && sync.skipped.is_empty() {
        println!("dist/ already holds exactly the active plugins.");
    } else if dry_run {
        println!("Nothing was changed (--dry-run).");
    }
}
/// Folders under dist/ gitpm leaves alone, and active plugins left without dist/.
fn warn_dist(sync: &gitpm::DistSync) {
    for path in &sync.unmanaged {
        eprintln!("Warning: {path} belongs to no known plugin or is not a folder; left as it is");
    }
    for skipped in &sync.skipped {
        eprintln!(
            "Warning: {}: dist not deployed ({}); run `gitpm sync-dist`",
            skipped.id, skipped.reason
        );
    }
}
/// `ID updated A → B` or `ID up to date (A)`.
fn print_change(change: &gitpm::RevisionChange) {
    warn(&change.warnings);
    let short = |commit: &str| commit.chars().take(12).collect::<String>();
    if !change.changed {
        println!("{} up to date ({})", change.id, change.to.version);
    } else if change.from.version == change.to.version {
        println!(
            "{} updated {} ({} → {})",
            change.id,
            change.to.version,
            short(&change.from.commit),
            short(&change.to.commit)
        );
    } else {
        println!(
            "{} updated {} → {}",
            change.id, change.from.version, change.to.version
        );
    }
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
            "Forked: {} → {} ({})",
            package.original,
            package.id,
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
/// replace (one package or several, same output). The fork is never removed; every
/// package that did not finish says how to finish or undo.
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
    let quoted = scaffold::shell_quote(&outcome.fork.path.to_string_lossy());
    let mut first_error = None;
    for package in outcome.packages {
        let (original, new_id) = (&package.original, &package.id);
        if let Some(tag) = &package.tag {
            println!("Released: {tag}");
        }
        if let Some(installed) = &package.installed {
            println!("Installed: {new_id} {}", installed.current.version);
        }
        let error = match (package.failed, package.error) {
            (None, _) => {
                if let Some(plan) = &package.replacement {
                    print_replacement(manager, plan)?;
                }
                continue;
            }
            (
                Some(gitpm::ForkStep::Replace),
                Some(error @ PmError::IncompatibleReplacement { .. }),
            ) if !force => {
                let plan = manager.replace_plan(original, new_id)?;
                if confirm_replacement(&plan, false)? {
                    print_replacement(manager, &manager.replace(original, new_id, true)?)?;
                    continue;
                }
                error
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
    "create",
    "fork",
    "remote",
    "auth",
    "info",
    "catalog",
    "catalog-update",
    "migrate-catalog",
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
const USAGE: &str = "gitpm [--root DIR] [--catalog DIR_OR_JSON_OR_URL] COMMAND

Workspace (installed plugins):
  install GIT_SOURCE [ID] [--ref REF]  install the latest Git release (tag); --ref picks a
                                   tag, branch or commit; --channel dev follows the default
                                   branch. ID (or --package ID) picks a package of a
                                   repository with gitpm.json (needed when it has several)
  install-catalog ID [--version V]  install an exact catalog release (latest if omitted)
  list [--provides SERVICE]        installed state (JSON); --provides keeps only packages
                                   whose plugin.json provides SERVICE
  check                            compare installed versions with the catalog
  update ID [--ref REF]             update to the latest Git release; --channel dev follows
                                   origin HEAD
  update all [--dry-run]           jointly upgrade compatible releases; preview with --dry-run
  rollback ID                      swap current and previous version
  enable ID | disable ID [--force]  deploy / remove dist/ID; disable refuses while other
                                   installed plugins depend on ID unless --force
  forget ID [--force] | restore ID  unregister (keeps the clone, removes dist/ID) /
                                   re-register a retained clone
  uninstall ID [--force]            remove state, repository and dist; asks on a TTY
                                   before losing local work and refuses while others
                                   depend on ID; --force skips both
  fork ID [ID...] --ns NS --into DIR  fork installed packages of one repository as
                                   NS/<same name> into the fork repository DIR: a new
                                   folder becomes a copy of the source (history, local
                                   edits; unforked sibling packages removed), a repository
                                   with gitpm.json gets the packages added; anything else
                                   is refused. Dependencies between forks are rewritten
    [--replace [--version V] [--force]]  then release each fork (tag NS/NAME@V, V defaults
                                   to the forked version), install it and replace ID with
                                   it for every consumer
  replace ORIGINAL --with FORK [--force]  make FORK provide ORIGINAL for every consumer;
                                   removes dist/ORIGINAL (FORK keeps its own dist/)
  replace ORIGINAL --back [--force]  switch every consumer back to the installed ORIGINAL
                                   and deploy dist/ORIGINAL again
                                   (incompatible consumers: asks on a TTY, else --force)
  sync-dist [--dry-run]            make dist/ hold exactly the active plugins (installed,
                                   enabled, not replaced); run once after upgrading a
                                   workspace from gitpm 0.4 (mutating commands
                                   reconcile dist/ automatically)

Authoring:
  init NAMESPACE/NAME --into DIR   create the package with release 0.1.0 (init = create):
  create NAMESPACE/NAME --into DIR  a new DIR becomes a single-package repository; a
                                   repository DIR with gitpm.json (or without one and no
                                   root package) gets the folder PREFIX/NAME
    [--multi] [--no-release]       --multi: a new (or empty) DIR becomes a multi-package
                                   repository (gitpm.json, packages/NAME); --no-release:
                                   commit only, no tag (release later with commit)
  commit REPO VERSION              stage ALL changes, commit, create a release tag;
    [--package ID]                 --package picks the package of a multi-package repo
  remote REPO URL                  add or replace origin (fetch and push)
  auth check GIT_URL               check repository read access without installing

Catalog (a local folder of static JSON files, or an HTTP(S) URL for reading):
  info                             show the catalog's directories and plugin versions
  catalog                          show directory links at this catalog level (JSON)
  catalog-update [--sources FILE]   generate/refresh a local catalog from the Git release
                                   tags of the repositories listed in FILE
  register GIT_SOURCE --ref REF     add one committed version to a local catalog
    [--package ID]                 package of a multi-package repository
    [--build-url HTTPS_URL --build-sha256 SHA256]  optional build metadata
  migrate-catalog                  split a local v1 catalog into plugin folders

Settings (flag, then environment, then default; there is no config file):
  --root DIR      GITPM_ROOT       workspace directory; default ./gitpm-workspace
  --catalog LOC   GITPM_CATALOG    catalog folder, catalog.json or URL; no default
                                   (needed by catalog commands, install-catalog, check)
  --sources FILE  GITPM_SOURCES    Git sources list for catalog-update; default ./sources.txt

Nothing is built or executed and nothing is pushed or uploaded: gitpm clones Git
repositories and copies their dist/ folder.";
fn run() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root = option(&mut args, "--root")?
        .or_else(|| std::env::var("GITPM_ROOT").ok())
        .unwrap_or(DEFAULT_ROOT.into());
    let catalog_location = option(&mut args, "--catalog")?
        .or_else(|| std::env::var("GITPM_CATALOG").ok())
        .filter(|location| !location.is_empty());
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    if matches!(args[0].as_str(), "--version" | "-V") {
        println!("gitpm {}", env!("CARGO_PKG_VERSION"));
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
    ensure!(
        channel.is_none() || reference.is_none(),
        "--channel conflicts with --ref"
    );
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
        into.is_none() || matches!(command, "init" | "create" | "fork"),
        "--into requires init/create/fork"
    );
    ensure!(
        provides.is_none() || command == "list",
        "--provides requires list"
    );
    ensure!(
        (!multi && !no_release) || matches!(command, "init" | "create"),
        "--multi/--no-release require init/create"
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
        "info" | "catalog-update" | "migrate-catalog" | "list" | "check" | "catalog"
        | "sync-dist" => 1,
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
    if matches!(command, "init" | "create") {
        split_id(id)?;
        ensure!(
            into.is_some(),
            "{command} needs --into DIR: the repository (a new folder, or a repository with gitpm.json to add the package to)"
        );
    }
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
    if let Some(package) = &package {
        gitpm::valid_id(package)?;
    }
    let update_all = command == "update" && id == "all";
    ensure!(
        !dry_run || update_all || command == "sync-dist",
        "--dry-run requires update all or sync-dist"
    );
    ensure!(
        !update_all || (reference.is_none() && !dev),
        "update all supports stable releases only; --ref and --channel dev are not supported"
    );
    // Validate input before opening the workspace: a rejected command creates nothing.
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
        for (index, other) in args[1..].iter().enumerate() {
            gitpm::valid_id(other)?;
            ensure!(
                !args[1..index + 1].contains(other),
                "{other} is given more than once"
            );
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
        "migrate-catalog" => {
            catalog::migrate(&require_catalog()?)?;
            return Ok(());
        }
        "init" | "create" => {
            let (namespace, name) = split_id(id)?;
            let dir = into.as_deref().unwrap_or_default();
            let options = scaffold::InitOptions {
                multi,
                release: !no_release,
            };
            let created = scaffold::init_with(Path::new(dir), namespace, name, options)?;
            let plugin_id = identity::qualified(namespace, name)?;
            let source = scaffold::shell_quote(&created.repo.to_string_lossy());
            let root = scaffold::shell_quote(&root);
            let package = match &created.folder {
                None => String::new(),
                Some(_) => format!(" --package {plugin_id}"),
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
                    "\nCreated: {}\nID: {plugin_id}\nVersion 0.1.0: {version}.\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source}\n\nNew release after editing:\n  gitpm commit {source} 0.1.1\n\nGit uses your configured identity; if there is none, the repository gets a local\nPlugin Demo <demo@example.invalid>.",
                    created.repo.display(),
                    created.repo.display()
                ),
                Some(folder) => {
                    let path = created.repo.join(folder);
                    println!(
                        "\nCreated: {}\nID: {plugin_id}\nVersion 0.1.0: {version} (only gitpm.json and this folder are committed).\nEdit: {}/dist/index.js\n\nInstall:\n{install}  gitpm --root {root} install {source} {plugin_id}\n\nNew release after editing (stages ALL changes of the repository):\n  gitpm commit {source} 0.1.1{package}",
                        path.display(),
                        path.display()
                    );
                }
            }
            return Ok(());
        }
        "check" => {
            let state = check::read_state(Path::new(&root))?;
            if state.plugins.is_empty() {
                println!("No plugins installed in workspace {root}.");
            } else {
                let url = require_catalog()?;
                let rows = check::check(Path::new(&root), &url)?;
                println!("Workspace: {root}\nCatalog: {url}\n");
                let color = io::stdout().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none()
                    && std::env::var("TERM").as_deref() != Ok("dumb");
                print!("{}", check_text::display_colored(&rows, color));
                for (original, fork) in &state.replacements {
                    println!("Replacement: {original} → {fork} (for every consumer)");
                }
                for plugin in state.installed() {
                    for dep in plugin.dependencies.iter().filter(|d| !d.satisfied) {
                        println!(
                            "Unsatisfied dependency: {} → {} {}: {}",
                            plugin.id,
                            dep.id,
                            dep.requirement,
                            dep.problem.as_deref().unwrap_or("")
                        );
                    }
                }
            }
            return Ok(());
        }
        "catalog" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&catalog::list(&require_catalog()?)?)?
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
            let tag = scaffold::commit_package(Path::new(id), &args[2], package.as_deref())?;
            println!("Created local commit and tag {tag}.");
            return Ok(());
        }
        "install-catalog" => {
            require_catalog()?;
        }
        _ => {}
    }
    // A direct Git install needs a catalog only for missing dependencies; a local
    // catalog that does not exist means none is configured (the library says so).
    let catalog = catalog_location.filter(|url| {
        let is_url = url.starts_with("https://") || url.starts_with("http://");
        command != "install" || is_url || Path::new(url).exists()
    });
    let manager = Manager::open_with(root, Options { catalog })?;
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
        "list" => {
            // Raw state plus the resolved view (providers, dependents, replacements).
            let mut state = manager.state()?;
            let mut resolved = manager.list()?;
            if let Some(service) = &provides {
                // Only packages whose plugin.json provides the service.
                resolved.retain(|p| p.provides.contains(service));
                state
                    .plugins
                    .retain(|id, _| resolved.iter().any(|p| &p.id == id));
            }
            let mut value = serde_json::to_value(state)?;
            value["resolved"] = serde_json::to_value(resolved)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        "install" => {
            let result =
                manager.install_package(id, package.as_deref(), reference.as_deref(), dev)?;
            for skipped in &result.skipped {
                eprintln!(
                    "Warning: skipped release tag {}: {}",
                    skipped.name, skipped.reason
                );
            }
            print_installed(&result);
        }
        "install-catalog" => {
            print_installed(&manager.install_from_catalog(id, version.as_deref())?);
        }
        "update" => {
            if update_all {
                let before = manager.state()?;
                let plan = if dry_run {
                    manager.update_all_plan()?
                } else {
                    manager.update_all()?
                };
                print_update_all(&before, &plan, dry_run);
            } else {
                print_change(&manager.update_channel(id, reference.as_deref(), dev)?);
            }
        }
        "rollback" => {
            manager.rollback(id)?;
        }
        "enable" => manager.set_enabled(id, true)?,
        "disable" => manager.set_enabled_with(id, false, force)?,
        "forget" => manager.forget_with(id, force)?,
        "replace" => {
            let plan = match &with {
                Some(fork) => manager.replace_plan(id, fork)?,
                None => manager.switch_back_plan(id)?,
            };
            let confirm = confirm_replacement(&plan, force)?;
            match &with {
                Some(fork) => print_replacement(&manager, &manager.replace(id, fork, confirm)?)?,
                None => print_switch_back(&manager, &manager.switch_back(id, confirm)?)?,
            }
        }
        "restore" => {
            manager.restore(id)?;
        }
        "sync-dist" => {
            let sync = if dry_run {
                manager.sync_dist_plan()?
            } else {
                manager.sync_dist()?
            };
            print_dist_sync(&sync, dry_run);
            if !dry_run && !sync.skipped.is_empty() {
                let failed: Vec<&str> = sync.skipped.iter().map(|s| s.id.as_str()).collect();
                bail!(
                    "sync-dist: {} package(s) not deployed: {}",
                    failed.len(),
                    failed.join(", ")
                );
            }
        }
        "uninstall" => {
            if force {
                manager.uninstall_with(id, true, true)?;
            } else {
                let plan = manager.uninstall_plan(id)?;
                if !plan.dependents.is_empty() {
                    // Report local work too, so one refusal tells everything.
                    for item in &plan.lost_work {
                        eprintln!("Local work that would be lost: {}", item.describe());
                    }
                    manager.uninstall(id, false)?;
                }
                if plan.lost_work.is_empty() || !io::stdin().is_terminal() {
                    // Without a TTY the library refuses and lists the local work.
                    manager.uninstall(id, false)?;
                } else if confirm_lost_work(&plan)? {
                    manager.uninstall(id, true)?;
                } else {
                    bail!("uninstall cancelled; nothing was changed");
                }
            }
        }
        _ => bail!("unknown command: {command}"),
    }
    // Mutating commands reconcile dist/ (already committed): report what they left alone.
    let mutating = matches!(
        command,
        "install"
            | "install-catalog"
            | "update"
            | "rollback"
            | "enable"
            | "disable"
            | "forget"
            | "restore"
            | "uninstall"
            | "replace"
    ) || (command == "fork" && replace);
    if mutating && !dry_run {
        match manager.sync_dist_plan() {
            Ok(sync) => warn_dist(&sync),
            Err(error) => eprintln!("Warning: cannot check dist/: {error}"),
        }
    }
    // These print what they did instead.
    if !matches!(
        command,
        "list" | "install" | "install-catalog" | "update" | "sync-dist"
    ) {
        println!("OK: {command} {id}");
    }
    Ok(())
}
