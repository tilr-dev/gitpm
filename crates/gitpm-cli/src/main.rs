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
];
const DEFAULT_ROOT: &str = "gitpm-workspace";
const DEFAULT_SOURCES: &str = "sources.txt";
const USAGE: &str = "gitpm [--root DIR] [--catalog DIR_OR_JSON_OR_URL] COMMAND

Workspace (installed plugins):
  install GIT_SOURCE [--ref REF]    install the latest Git release (tag); --ref picks a tag,
                                   branch or commit; --channel dev follows the default branch
  install-catalog ID [--version V]  install an exact catalog release (latest if omitted)
  list                             installed state (JSON)
  check                            compare installed versions with the catalog
  update ID [--ref REF]             update to the latest Git release; --channel dev follows
                                   origin HEAD
  update all [--dry-run]           jointly upgrade compatible releases; preview with --dry-run
  rollback ID                      swap current and previous version
  enable ID | disable ID [--force]  change state only; disable refuses while other
                                   installed plugins depend on ID unless --force
  forget ID [--force] | restore ID  unregister / re-register retained files
  uninstall ID [--force]            remove state, repository and dist; asks on a TTY
                                   before losing local work and refuses while others
                                   depend on ID; --force skips both
  fork ID NAMESPACE/NAME           copy an installed plugin to the new folder
                                   ./NAMESPACE/NAME with that ID; NAME must equal the name
                                   part of ID
  replace ORIGINAL --with FORK [--force]  make FORK provide ORIGINAL for every consumer
  replace ORIGINAL --back [--force]  switch every consumer back to the installed ORIGINAL
                                   (incompatible consumers: asks on a TTY, else --force)

Authoring:
  init NAMESPACE/NAME              create the plugin repository ./NAMESPACE/NAME with
  create NAMESPACE/NAME            that ID and release 0.1.0 (init = create)
  commit REPO VERSION              stage ALL changes, commit, create a release tag
  remote REPO URL                  add or replace origin (fetch and push)
  auth check GIT_URL               check repository read access without installing

Catalog (a local folder of static JSON files, or an HTTP(S) URL for reading):
  info                             show the catalog's directories and plugin versions
  catalog                          show directory links at this catalog level (JSON)
  catalog-update [--sources FILE]   generate/refresh a local catalog from the Git release
                                   tags of the repositories listed in FILE
  register GIT_SOURCE --ref REF     add one committed version to a local catalog
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
        version.is_none() || command == "install-catalog",
        "--version is not valid for {command}"
    );
    ensure!(
        !force || matches!(command, "uninstall" | "disable" | "forget" | "replace"),
        "--force is only valid with uninstall/disable/forget/replace"
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
        "info" | "catalog-update" | "migrate-catalog" | "list" | "check" | "catalog" => 1,
        "auth" | "commit" | "remote" | "fork" => 3,
        _ => 2,
    };
    ensure!(
        args.len() == expected,
        "{command}: expected {} argument(s), got {}; use --help",
        expected - 1,
        args.len() - 1
    );
    let id = args.get(1).map(String::as_str).unwrap_or("");
    let update_all = command == "update" && id == "all";
    ensure!(!dry_run || update_all, "--dry-run requires update all");
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
        let (_, name) = split_id(&args[2])?;
        let source_name = id.rsplit('/').next().unwrap_or(id);
        ensure!(
            name == source_name,
            "fork keeps the plugin name: use NAMESPACE/{source_name}, not {}",
            args[2]
        );
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
            let path = scaffold::create_namespaced(&Path::new(namespace).join(name), namespace)?;
            let plugin_id = identity::qualified(namespace, name)?;
            let tag = identity::release_tag(&plugin_id, "0.1.0");
            let source = scaffold::shell_quote(&path.to_string_lossy());
            let root = scaffold::shell_quote(&root);
            println!(
                "\nCreated: {}\nID: {plugin_id}\nVersion 0.1.0: Git commit and tag {tag}.\nEdit: {}/dist/index.js\n\nInstall:\n  gitpm --root {root} install {source}\n\nNew release after editing:\n  gitpm commit {source} 0.1.1\n\nGit uses your configured identity; if there is none, the repository gets a local\nPlugin Demo <demo@example.invalid>.",
                path.display(),
                path.display()
            );
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
                serde_json::to_string_pretty(&catalog::register_build(
                    &url, id, reference, build
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
            let tag = scaffold::commit(Path::new(id), &args[2])?;
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
            let (namespace, name) = split_id(&args[2])?;
            let path =
                manager.fork_in_namespace(id, &Path::new(namespace).join(name), namespace)?;
            let new_id = identity::qualified(namespace, name)?;
            println!(
                "Fork created: {}\nGit history and local edits are kept; the original repository is the upstream remote.\nNew ID: {new_id}. forkOf records the origin; replaces says the fork can stand in for {id}\n(used only after an explicit: gitpm replace {id} --with {new_id}).\nThe ID change is committed separately. Commit local edits before installing the fork.\nThe original installation is unchanged.\nSet your own origin with: gitpm remote {} GIT_URL",
                path.display(),
                scaffold::shell_quote(&path.to_string_lossy())
            );
        }
        "list" => {
            // Raw state plus the resolved view (providers, dependents, replacements).
            let mut value = serde_json::to_value(manager.state()?)?;
            value["resolved"] = serde_json::to_value(manager.list()?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        "install" => {
            let result = manager.install_channel(id, reference.as_deref(), dev)?;
            for skipped in &result.skipped {
                eprintln!(
                    "Warning: skipped release tag {}: {}",
                    skipped.name, skipped.reason
                );
            }
        }
        "install-catalog" => {
            manager.install_from_catalog(id, version.as_deref())?;
        }
        "update" => {
            if update_all {
                let plan = if dry_run {
                    manager.update_all_plan()?
                } else {
                    manager.update_all()?
                };
                for change in &plan.updates {
                    println!(
                        "Update {}: {} -> {}",
                        change.id, change.from.version, change.to.version
                    );
                }
                for install in &plan.installs {
                    println!("Install {}: {}", install.id, install.current.version);
                }
                for held in &plan.kept_back {
                    println!(
                        "Kept back {}: selected {}, latest {} (dependency constraints)",
                        held.id, held.selected, held.latest
                    );
                }
                println!(
                    "{} updates, {} installs, {} kept back.{}",
                    plan.updates.len(),
                    plan.installs.len(),
                    plan.kept_back.len(),
                    if dry_run { " Nothing was changed." } else { "" }
                );
            } else {
                manager.update_channel(id, reference.as_deref(), dev)?;
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
            let plan = match &with {
                Some(fork) => manager.replace(id, fork, confirm)?,
                None => manager.switch_back(id, confirm)?,
            };
            println!(
                "{} → {} ({}) for: {}",
                plan.original,
                plan.provider,
                plan.version,
                if plan.consumers.is_empty() {
                    "no consumers yet".to_owned()
                } else {
                    plan.consumers.join(", ")
                }
            );
        }
        "restore" => {
            manager.restore(id)?;
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
    if command != "list" {
        println!("OK: {command} {id}");
    }
    Ok(())
}
