use std::path::{Path, PathBuf};

/// Expected source-keyed clone destination, independent of registration lifetime.
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
