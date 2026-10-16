//! Text rendering for the `gitpm` command-line client.
//! All business logic lives in the `gitpm` library; this crate only adds presentation.

/// Text overview of a catalog, as printed by `gitpm info`.
pub mod catalog {
    use gitpm::catalog::{
        CatalogFormat, CatalogOverview, OVERVIEW_LIMIT, OverviewItem, PluginSummary, browse,
    };
    use std::fmt::Write as _;

    /// Browse a catalog (local folder, JSON file or HTTP URL) and render it as text.
    pub fn info(location: &str) -> gitpm::Result<String> {
        Ok(render_overview(&browse(location)?))
    }

    fn summary(plugin: &PluginSummary) -> String {
        format!(
            "{} — latest: {}, versions: {}\n    Git: {}",
            plugin.id,
            plugin.latest.as_deref().unwrap_or("no releases"),
            plugin.versions.len(),
            plugin.source
        )
    }

    pub fn render_overview(overview: &CatalogOverview) -> String {
        let mut output = format!(
            "Catalog: {}\nType: {}\n",
            overview.location,
            if overview.remote {
                "remote (HTTP)"
            } else {
                "local folder/JSON"
            }
        );
        match overview.format {
            CatalogFormat::V1 => {
                output.push_str("Format: v1 — single JSON file, no directory tree\nPlugins:\n");
                for item in &overview.items {
                    match item {
                        OverviewItem::Plugin { plugin, .. } => {
                            let _ = writeln!(output, "  {}", summary(plugin));
                        }
                        OverviewItem::Empty { .. } => output.push_str("  (empty)\n"),
                        OverviewItem::Directory { .. } => {}
                    }
                }
            }
            CatalogFormat::V2 => {
                output.push_str("Format: v2\nDirectories and plugins:\n");
                for item in &overview.items {
                    match item {
                        OverviewItem::Empty { depth } => {
                            let _ = writeln!(output, "{}(empty)", "  ".repeat(*depth as usize));
                        }
                        OverviewItem::Directory { depth, name } => {
                            let _ = writeln!(output, "{}{name}/", "  ".repeat(*depth as usize));
                        }
                        OverviewItem::Plugin { depth, plugin } => {
                            let indent = "  ".repeat(*depth as usize);
                            for line in summary(plugin).lines() {
                                let _ = writeln!(output, "{indent}  {line}");
                            }
                        }
                    }
                }
            }
        }
        if overview.truncated {
            let _ = writeln!(
                output,
                "Overview limit: {OVERVIEW_LIMIT} entries. To browse one branch: gitpm --catalog <branch location> info"
            );
        }
        output
    }
}

/// The table printed by `gitpm check`.
pub mod check {
    use gitpm::check::CheckRow;

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
        let latest = |row: &CheckRow| row.latest.clone().unwrap_or_else(|| "—".into());
        let mut output = format!(
            "{:<24} {:<14} {:<14} {:<8} STATUS\n",
            "PLUGIN", "INSTALLED", "LATEST", "ENABLED"
        );
        for row in rows {
            output.push_str(&format!(
                "{:<24} {:<14} {} {:<8} {}\n",
                row.id,
                row.installed,
                if row.update_available {
                    green(format!("{:<14}", latest(row)))
                } else {
                    format!("{:<14}", latest(row))
                },
                if row.enabled { "yes" } else { "no" },
                if row.update_available {
                    green(row.status.message().to_owned())
                } else {
                    row.status.message().to_owned()
                }
            ));
        }
        for row in rows {
            if let Some(upstream) = &row.upstream {
                let latest = upstream.latest.as_deref().unwrap_or("not in catalog");
                output.push_str(&format!(
                    "\n{}: fork of {} (base {}); original in catalog: {}{}",
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
                update_available,
                status: if update_available {
                    CheckStatus::UpdateAvailable
                } else {
                    CheckStatus::UpToDate
                },
            }
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
    }
}
