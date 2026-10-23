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
struct RawGitBoundary {
    found: Vec<String>,
    forbid_http: bool,
}

/// Macro bodies are arbitrary token trees, not Rust expressions. Visit their
/// identifiers recursively without interpreting string literals as source code.
struct MacroIdentifiers(Vec<syn::Ident>);
impl syn::parse::Parse for MacroIdentifiers {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        use syn::ext::IdentExt;
        let mut identifiers = Vec::new();
        while !input.is_empty() {
            let content;
            if input.peek(syn::Ident::peek_any) {
                identifiers.push(input.call(syn::Ident::parse_any)?);
                continue;
            } else if input.peek(syn::token::Paren) {
                syn::parenthesized!(content in input);
            } else if input.peek(syn::token::Brace) {
                syn::braced!(content in input);
            } else if input.peek(syn::token::Bracket) {
                syn::bracketed!(content in input);
            } else {
                input.step(|cursor| Ok(((), cursor.token_tree().unwrap().1)))?;
                continue;
            }
            identifiers.extend(content.parse::<Self>()?.0);
        }
        Ok(Self(identifiers))
    }
}
impl<'ast> Visit<'ast> for RawGitBoundary {
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
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.visit_path(&item.path);
        for ident in syn::parse2::<MacroIdentifiers>(item.tokens.clone())
            .unwrap()
            .0
        {
            self.visit_ident(&ident);
        }
    }
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        let name = ident.to_string();
        if matches!(name.as_str(), "git" | "git_raw" | "git_with_input")
            || (self.forbid_http && name == "http_get")
        {
            self.found.push(name);
        }
    }
}
fn raw_git_violations(source: &str, forbid_http: bool) -> Vec<String> {
    let mut boundary = RawGitBoundary {
        forbid_http,
        ..Default::default()
    };
    boundary.visit_file(&syn::parse_file(source).unwrap());
    boundary.found
}
#[test]
fn workspace_uses_typed_transport() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["workspace.rs", "workspace/clones.rs", "workspace/fork.rs"] {
        let found = raw_git_violations(&fs::read_to_string(root.join(file)).unwrap(), true);
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
        "fn run() { ensure!(crate::transport::git(repo, &[\"cat-file\", \"-t\", &tree], ssh).is_ok()); }",
        "fn run() { outer!({ inner!([git_raw()]); }); }",
    ] {
        assert!(!raw_git_violations(source, true).is_empty(), "{source}");
    }
    assert!(
        raw_git_violations(
            "#[cfg(test)] mod tests { use crate::test_support::git; fn setup() { git(); } }",
            true
        )
        .is_empty()
    );
    assert!(raw_git_violations("fn run() { SystemTransport.object_id(); }", true).is_empty());
    assert!(raw_git_violations("fn run() { ensure!(ok, \"git\"); }", true).is_empty());
}
fn check_typed_tree(path: &Path) {
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            check_typed_tree(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|e| e == "rs") {
        let found = raw_git_violations(&fs::read_to_string(path).unwrap(), false);
        assert!(
            found.is_empty(),
            "{} uses raw Git outside transport: {found:?}",
            path.display()
        );
    }
}
#[test]
fn releases_and_catalog_use_typed_transport() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in [
        "releases",
        "lost_work",
        "catalog",
        "check",
        "model",
        "packages",
        "auth",
    ] {
        let file = src.join(format!("{module}.rs"));
        let directory = src.join(module);
        assert!(
            file.exists() || directory.exists(),
            "missing module {module}"
        );
        if file.exists() {
            check_typed_tree(&file);
        }
        if directory.exists() {
            check_typed_tree(&directory);
        }
    }
}
#[test]
fn typed_boundary_detects_raw_calls_and_import_aliases() {
    assert!(raw_git_violations("fn run() { backend.http_get(); }", false).is_empty());
    for source in [
        "use crate::transport::git as run;",
        "use crate::transport::{git_raw as read};",
        "fn run() { crate::transport::git_with_input(); }",
        "fn run(t: impl Transport) { t.git(); }",
    ] {
        assert!(!raw_git_violations(source, false).is_empty(), "{source}");
    }
    assert!(
        raw_git_violations(
            "#[cfg(test)] mod tests { use crate::test_support::git; fn fixture() { git(); } }",
            false
        )
        .is_empty()
    );
}
