//! Parse production source to keep process creation inside transport.
use std::{fs, path::Path};
use syn::visit::{self, Visit};

#[derive(Default)]
struct Boundary(Vec<String>);
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Path>()
                .is_ok_and(|p| p.is_ident("test"))
    })
}
impl<'ast> Visit<'ast> for Boundary {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.first().is_some_and(|s| s.ident == "libc") {
            let name = path.segments.last().unwrap().ident.to_string();
            if matches!(
                name.as_str(),
                "posix_spawn"
                    | "posix_spawnp"
                    | "execve"
                    | "execvp"
                    | "execl"
                    | "fork"
                    | "vfork"
                    | "system"
                    | "popen"
            ) {
                self.0.push(name);
            }
        }
        visit::visit_path(self, path);
    }
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if matches!(name.as_str(), "Command" | "Stdio") {
            self.0.push(name);
        }
    }
}
fn violations(source: &str) -> Vec<String> {
    let mut boundary = Boundary::default();
    boundary.visit_file(&syn::parse_file(source).unwrap());
    boundary.0
}
fn check_tree(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "transport"
            || path.file_name().unwrap() == "test_support.rs"
        {
            continue;
        }
        if path.is_dir() {
            check_tree(&path);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let found = violations(&fs::read_to_string(&path).unwrap());
            assert!(
                found.is_empty(),
                "{} starts processes outside transport: {found:?}",
                path.display()
            );
        }
    }
}
#[test]
fn production_processes_stay_inside_transport() {
    check_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}
#[test]
fn boundary_detects_imports_aliases_and_qualified_calls() {
    for source in [
        "use std::process::Command as Runner;",
        "use std::{process::{Command as Runner}};",
        "fn run() { std::process::Command::new(\"git\"); }",
        "fn run() { unsafe { libc::execve(); } }",
    ] {
        assert!(!violations(source).is_empty(), "{source}");
    }
    assert!(violations("#[cfg(test)] mod tests { use std::process::Command; }").is_empty());
}

/// The staged migration permits raw helpers elsewhere, but workspace callers
/// must express operations through the typed transport API. Checking identifiers
/// also catches imports renamed locally and inherited calls through `super::*`.
#[derive(Default)]
struct WorkspaceBoundary(Vec<String>);
impl<'ast> Visit<'ast> for WorkspaceBoundary {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if matches!(
            name.as_str(),
            "git" | "git_raw" | "git_with_input" | "http_get"
        ) {
            self.0.push(name);
        }
    }
}
fn workspace_violations(source: &str) -> Vec<String> {
    let mut boundary = WorkspaceBoundary::default();
    boundary.visit_file(&syn::parse_file(source).unwrap());
    boundary.0
}
#[test]
fn workspace_uses_typed_transport() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["workspace.rs", "workspace/clones.rs", "workspace/fork.rs"] {
        let found = workspace_violations(&fs::read_to_string(root.join(file)).unwrap());
        assert!(found.is_empty(), "{file} uses raw transport: {found:?}");
    }
}
#[test]
fn workspace_boundary_detects_raw_calls_and_imports() {
    for source in [
        "use crate::transport::git as run;",
        "use crate::{transport::{git_raw as read}};",
        "fn run() { crate::transport::git_with_input(); }",
        "use super::*; fn run() { git(); }",
        "fn run() { backend.git(); }",
        "fn run() { crate::transport::http_get(); }",
    ] {
        assert!(!workspace_violations(source).is_empty(), "{source}");
    }
    assert!(
        workspace_violations(
            "#[cfg(test)] mod tests { use crate::test_support::git; fn setup() { git(); } }"
        )
        .is_empty()
    );
    assert!(workspace_violations("fn run() { SystemTransport.head_commit(); }").is_empty());
}
