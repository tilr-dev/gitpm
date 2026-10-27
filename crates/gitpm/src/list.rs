//! Read-only list view of a workspace: the raw `state.json` plus the resolved packages,
//! read without opening (locking, creating or recovering) the workspace.
use crate::{InstalledPackage, Result, State, check::read_state};
use serde::Serialize;
use std::path::Path;

/// What [`list_with`] keeps.
#[derive(Clone, Debug, Default)]
pub struct ListOptions {
    /// Keep only the packages whose manifest `provides` this service (in `packages` and
    /// `resolved`; `replacements` and `forgotten` stay unfiltered, and inactive providers
    /// are kept: the list is a status view).
    pub provides: Option<String>,
}

/// The list view: the raw state (`format`, `packages`, `replacements`, `forgotten`)
/// with `resolved` beside it, as one JSON object.
#[derive(Clone, Debug, Serialize)]
pub struct ListView {
    #[serde(flatten)]
    pub state: State,
    /// [`State::installed`]: every installed package with `active`, metadata, dependency
    /// status, dependents and replacements.
    pub resolved: Vec<InstalledPackage>,
}

/// Read the list view of the workspace at `root`. `state.json` is replaced by an atomic
/// rename, so this sees the last committed state even while another process holds the
/// workspace lock; an interrupted transaction is completed only by the next command that
/// opens the workspace. A missing workspace lists as empty and is not created.
pub fn list_with(root: &Path, options: &ListOptions) -> Result<ListView> {
    let mut state = read_state(root)?;
    let mut resolved = state.installed();
    if let Some(service) = &options.provides {
        resolved.retain(|p| p.provides.contains(service));
        state
            .packages
            .retain(|id, _| resolved.iter().any(|p| &p.id == id));
    }
    Ok(ListView { state, resolved })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir, state_with, tree_hash};
    use std::fs;

    #[test]
    fn list_with_never_creates_or_locks_a_workspace() {
        let dir = TempDir::new("list");
        let root = dir.join("workspace");
        let view = list_with(&root, &ListOptions::default()).unwrap();
        assert!(view.state.packages.is_empty() && view.resolved.is_empty());
        assert!(!root.exists() && !dir.join(".workspace.lock").exists());
        // While a Manager holds the lock, the list still reads the committed state.
        let author = Author::new(dir.join("author"), "ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc");
        let m = crate::Manager::open(&root).unwrap();
        m.install(author.source()).unwrap();
        let before = tree_hash(dir.path());
        let view = list_with(&root, &ListOptions::default()).unwrap();
        assert_eq!(view.resolved.len(), 1);
        assert!(view.resolved[0].active);
        assert_eq!(tree_hash(dir.path()), before, "list wrote something");
        // One JSON object: the raw state with `resolved` beside it.
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["format"], crate::STATE_FORMAT);
        assert!(json["packages"]["ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc"].is_object());
        assert_eq!(
            json["resolved"][0]["id"],
            "ab93b236-d66d-5fc1-9cbc-3eaa4cf4ddcc"
        );
        drop(m);
    }

    #[test]
    fn provides_filters_packages_and_resolved_but_not_replacements() {
        let dir = TempDir::new("list");
        let root = dir.join("workspace");
        fs::create_dir(&root).unwrap();
        let mut state = state_with(
            "f851f609-bb81-52d6-bfef-33798624782c",
            "s",
            "1.0.0",
            &"a".repeat(40),
        );
        state.packages.extend(
            state_with(
                "d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50",
                "s",
                "1.0.0",
                &"b".repeat(40),
            )
            .packages,
        );
        state.packages.extend(
            state_with(
                "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
                "s",
                "1.0.0",
                &"c".repeat(40),
            )
            .packages,
        );
        for id in [
            "f851f609-bb81-52d6-bfef-33798624782c",
            "d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50",
        ] {
            state.packages.get_mut(id).unwrap().metadata.provides = vec!["theme".into()];
        }
        state.replacements.insert(
            "f851f609-bb81-52d6-bfef-33798624782c".into(),
            "d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50".into(),
        );
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let options = ListOptions {
            provides: Some("theme".into()),
        };
        let view = list_with(&root, &options).unwrap();
        let ids: Vec<&str> = view.resolved.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50",
                "f851f609-bb81-52d6-bfef-33798624782c"
            ]
        );
        // A replaced provider is kept (status view), shown inactive.
        assert!(!view.resolved[1].active && view.resolved[0].active);
        assert_eq!(
            view.state.packages.keys().collect::<Vec<_>>(),
            [
                "d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50",
                "f851f609-bb81-52d6-bfef-33798624782c"
            ]
        );
        assert_eq!(view.state.replacements.len(), 1);
        let none = ListOptions {
            provides: Some("nothing".into()),
        };
        let view = list_with(&root, &none).unwrap();
        assert!(view.resolved.is_empty() && view.state.packages.is_empty());
        assert_eq!(
            view.state.replacements.len(),
            1,
            "replacements are unfiltered"
        );
    }
}
