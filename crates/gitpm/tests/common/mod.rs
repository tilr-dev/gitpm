use std::path::{Path, PathBuf};

/// Expected source-keyed clone destination, independent of registration lifetime.
#[allow(dead_code)]
pub fn clone_path(root: &Path, source: &Path) -> PathBuf {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let record = gitpm::Installed {
        source: source.to_string_lossy().into_owned(),
        ..Default::default()
    };
    root.join(record.clone_folder(""))
}

/// A committed author fixture whose UUID is fixed before its first release.
#[allow(dead_code)]
pub fn author(path: &Path, id: &str, name: &str) -> PathBuf {
    std::fs::create_dir_all(path.join("dist")).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.name", "Test"]);
    run(&["config", "user.email", "test@example.invalid"]);
    run(&["config", "commit.gpgsign", "false"]);
    run(&["config", "tag.gpgsign", "false"]);
    std::fs::write(
        path.join("plugin.json"),
        serde_json::json!({"id": id, "name": name, "version": "0.1.0"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        path.join("dist/index.js"),
        format!("export const greeting = \"Hello from {name} 0.1.0\";\n"),
    )
    .unwrap();
    std::fs::write(path.join(".gitignore"), ".DS_Store\nnode_modules/\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "Initial fixture"]);
    run(&["tag", "v0.1.0"]);
    path.canonicalize().unwrap()
}
