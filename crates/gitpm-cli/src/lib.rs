//! Text rendering for the `gitpm` command-line client.
//! All business logic lives in the `gitpm` library; this crate only adds presentation.

/// The version `gitpm --version` prints: the package version, plus `-dev+<short hash>`
/// when built from a Git checkout of this repository that is not at a release tag (see
/// `build.rs`; packaged builds, such as from crates.io, print the plain version).
pub fn version() -> String {
    format!(
        "{}{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("GITPM_BUILD_SUFFIX").unwrap_or_default()
    )
}

#[cfg(test)]
mod version_tests {
    #[test]
    fn version_starts_with_the_package_version() {
        let version = super::version();
        let suffix = version
            .strip_prefix(env!("CARGO_PKG_VERSION"))
            .expect("starts with CARGO_PKG_VERSION");
        assert!(
            suffix.is_empty() || suffix.starts_with("-dev+") && suffix.len() > "-dev+".len(),
            "{version}"
        );
    }
}

/// Text overview of a catalog, as printed by `gitpm catalog show`.
pub mod catalog {
    use gitpm::catalog::{CatalogOverview, OVERVIEW_LIMIT, OverviewItem, PackageSummary, browse};
    use std::fmt::Write as _;

    /// Browse a catalog (local folder, JSON file or HTTP URL) and render it as text.
    pub fn info(location: &str) -> gitpm::Result<String> {
        Ok(render_overview(&browse(location)?))
    }

    fn summary(package: &PackageSummary) -> String {
        let mut text = format!(
            "{} — latest: {}, versions: {}\n    Git: {}",
            package.id,
            package
                .latest
                .as_deref()
                .unwrap_or(if package.versions.is_empty() {
                    "no releases"
                } else {
                    "pre-releases only"
                }),
            package.versions.len(),
            package.source
        );
        // Metadata of the latest release, when it has any.
        if let Some(title) = &package.title {
            let _ = write!(text, "\n    Title: {title}");
        }
        if let Some(description) = &package.description {
            let _ = write!(text, "\n    Description: {description}");
        }
        if !package.provides.is_empty() {
            let _ = write!(text, "\n    Provides: {}", package.provides.join(", "));
        }
        if !package.tags.is_empty() {
            let _ = write!(text, "\n    Tags: {}", package.tags.join(", "));
        }
        text
    }

    pub fn render_overview(overview: &CatalogOverview) -> String {
        let mut output = format!(
            "Catalog: {}\nType: {}\nVerified: {}\n",
            overview.location,
            if overview.remote {
                "remote (HTTP)"
            } else {
                "local folder/JSON"
            },
            if overview.verified {
                "yes (protects the packages installed from it)"
            } else {
                "no (discovery only)"
            }
        );
        output.push_str("Directories and packages:\n");
        for item in &overview.items {
            match item {
                OverviewItem::Empty { depth } => {
                    let _ = writeln!(output, "{}(empty)", "  ".repeat(*depth as usize));
                }
                OverviewItem::Directory { depth, name } => {
                    let _ = writeln!(output, "{}{name}/", "  ".repeat(*depth as usize));
                }
                OverviewItem::Package { depth, package } => {
                    let indent = "  ".repeat(*depth as usize);
                    for line in summary(package).lines() {
                        let _ = writeln!(output, "{indent}  {line}");
                    }
                }
            }
        }
        if overview.truncated {
            let _ = writeln!(
                output,
                "Overview limit: {OVERVIEW_LIMIT} entries. To browse one branch: gitpm --catalog <branch location> catalog show"
            );
        }
        output
    }
}

/// The table printed by `gitpm check`.
pub mod check {
    use gitpm::Mode;
    use gitpm::check::{CheckRow, CheckSource};

    pub fn display(rows: &[CheckRow]) -> String {
        display_colored(rows, false)
    }

    pub fn display_colored(rows: &[CheckRow], color: bool) -> String {
        let green = |text: String| {
            if color {
                format!("\x1b[32m{text}\x1b[0m")
            } else {
                text
            }
        };
        // A branch row compares commits: its latest is the branch HEAD.
        let latest = |row: &CheckRow| match (&row.latest, &row.mode) {
            (Some(latest), _) if row.major => format!("{latest} (major)"),
            (Some(latest), _) => latest.clone(),
            (None, Mode::Branch(branch)) => format!("{branch} HEAD"),
            (None, _) => "—".into(),
        };
        let mut output = format!(
            "{:<24} {:<14} {:<14} {:<8} {:<8} STATUS\n",
            "PACKAGE", "INSTALLED", "LATEST", "ENABLED", "SOURCE"
        );
        for row in rows {
            let mut status = match &row.problem {
                Some(problem) => format!("{}: {problem}", row.message()),
                None => row.message(),
            };
            match &row.mode {
                Mode::Release => {}
                Mode::Link(_) => status.push_str("; dev-linked and unchanged"),
                Mode::Pinned(reference) => status.push_str(&format!(" ({reference})")),
                Mode::Branch(branch) => status.push_str(&format!(" (branch {branch})")),
            }
            // A newer pre-release is named, never offered.
            if let Some(prerelease) = &row.prerelease {
                status.push_str(&format!("; pre-release {prerelease}"));
            }
            output.push_str(&format!(
                "{:<24} {:<14} {} {:<8} {:<8} {}\n",
                row.id,
                row.installed,
                if row.update_available {
                    green(format!("{:<14}", latest(row)))
                } else {
                    format!("{:<14}", latest(row))
                },
                if row.enabled { "yes" } else { "no" },
                row.source.as_str(),
                if row.update_available {
                    green(status)
                } else {
                    status
                }
            ));
        }
        for row in rows {
            if let Some(upstream) = &row.upstream {
                let latest = upstream.latest.as_deref();
                let (place, latest) = match upstream.source {
                    Some(CheckSource::Catalog) => {
                        ("original in catalog", latest.unwrap_or("no versions"))
                    }
                    Some(CheckSource::Origin) => {
                        ("original at origin", latest.unwrap_or("unknown"))
                    }
                    None => ("original", "unknown"),
                };
                let unreachable;
                let (place, latest) = match &upstream.problem {
                    Some(problem) => {
                        unreachable = format!("unreachable ({problem})");
                        ("original", unreachable.as_str())
                    }
                    None => (place, latest),
                };
                output.push_str(&format!(
                    "\n{}: fork of {} (base {}); {place}: {}{}",
                    row.id,
                    upstream.id,
                    upstream.base,
                    latest,
                    if upstream.newer {
                        green(" — newer than the fork base".into())
                    } else {
                        String::new()
                    }
                ));
            }
        }
        if rows.iter().any(|r| r.upstream.is_some()) {
            output.push('\n');
        }
        let updates = rows.iter().filter(|r| r.update_available).count();
        output.push_str(&format!("\nUpdates: {updates}. Nothing was changed.\n"));
        if updates > 0 {
            output.push_str("\nAvailable updates:\n");
            for row in rows.iter().filter(|r| r.update_available) {
                output.push_str(&format!(
                    "  {}: {} → {}\n",
                    row.id,
                    row.installed,
                    green(latest(row))
                ));
            }
        }
        output
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use gitpm::check::CheckStatus;
        fn row(update_available: bool) -> CheckRow {
            CheckRow {
                id: "demo".into(),
                installed: "1.9.0".into(),
                latest: Some("1.10.0".into()),
                enabled: false,
                upstream: None,
                source: CheckSource::Catalog,
                mode: Mode::Release,
                problem: None,
                origin_latest: None,
                update_available,
                status: if update_available {
                    CheckStatus::UpdateAvailable
                } else {
                    CheckStatus::UpToDate
                },
                major: false,
                prerelease: None,
            }
        }
        #[test]
        fn table_marks_major_updates_and_names_pre_releases() {
            let mut major = row(true);
            major.installed = "1.2.0".into();
            major.latest = Some("2.0.0".into());
            major.major = true;
            major.prerelease = Some("2.1.0-rc.1".into());
            let text = display(&[major]);
            assert!(text.contains("2.0.0 (major)"), "{text}");
            assert!(
                text.contains("update available; pre-release 2.1.0-rc.1"),
                "{text}"
            );
            assert!(text.contains("demo: 1.2.0 → 2.0.0 (major)"), "{text}");
            assert!(!display(&[row(true)]).contains("(major)"));
        }
        #[test]
        fn table_lists_updates_and_colors_only_on_request() {
            let text = display(&[row(true)]);
            assert!(text.contains("Updates: 1."));
            assert!(text.contains("demo: 1.9.0 → 1.10.0"));
            assert!(text.contains("update available"));
            assert!(!text.contains('\x1b'));
            assert!(display_colored(&[row(true)], true).contains("\x1b[32m"));
            let text = display(&[row(false)]);
            assert!(text.contains("Updates: 0."));
            assert!(!text.contains("Available updates"));
        }
        #[test]
        fn table_shows_the_recorded_mode() {
            let mut pinned = row(false);
            pinned.status = CheckStatus::Pinned;
            pinned.latest = None;
            pinned.mode = Mode::Pinned("demo@1.9.0".into());
            let mut branch = row(true);
            branch.latest = None;
            branch.mode = Mode::Branch("main".into());
            let text = display(&[pinned, branch]);
            assert!(text.contains("pinned (demo@1.9.0)"), "{text}");
            assert!(text.contains("update available (branch main)"), "{text}");
            assert!(text.contains("demo: 1.9.0 → main HEAD"), "{text}");
        }
        #[test]
        fn table_names_the_source_and_fork_upstream_place() {
            use gitpm::check::Upstream;
            let mut fork = row(false);
            fork.id = "ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4".into();
            fork.source = CheckSource::Origin;
            let upstream = |latest: Option<&str>, source| Upstream {
                id: "7e6e8392-7be8-55ae-bd03-2b430b1c5d59".into(),
                base: "1.0.0".into(),
                latest: latest.map(Into::into),
                newer: latest.is_some(),
                source,
                problem: None,
            };
            fork.upstream = Some(upstream(Some("1.1.0"), Some(CheckSource::Origin)));
            let text = display(std::slice::from_ref(&fork));
            assert!(text.contains(" origin "), "{text}");
            assert!(
                text.contains("ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4: fork of 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (base 1.0.0); original at origin: 1.1.0 — newer than the fork base"),
                "{text}"
            );
            fork.upstream = Some(upstream(Some("1.1.0"), Some(CheckSource::Catalog)));
            assert!(
                display(std::slice::from_ref(&fork)).contains("original in catalog: 1.1.0 — newer")
            );
            let mut gone = upstream(None, Some(CheckSource::Origin));
            gone.problem = Some("The Git server is unreachable.".into());
            fork.upstream = Some(gone);
            assert!(
                display(std::slice::from_ref(&fork)).contains(
                    "(base 1.0.0); original: unreachable (The Git server is unreachable.)"
                )
            );
            fork.upstream = Some(upstream(None, None));
            assert!(
                display(std::slice::from_ref(&fork)).contains("(base 1.0.0); original: unknown")
            );
            fork.status = CheckStatus::OriginUnreachable;
            fork.problem = Some("The Git server is unreachable.".into());
            assert!(
                display(&[fork]).contains("origin unreachable: The Git server is unreachable.")
            );
        }
    }
}
