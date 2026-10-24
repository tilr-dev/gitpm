## Artifact and verification
A build is a gzip-compressed tar archive whose root is the deployed output folder, without a wrapper directory. Only regular files and directories are supported; symlinks, hardlinks, devices, absolute paths, traversal and duplicate paths are refused. Executable file bits are preserved. Downloaded bytes must match build.sha256; extracted regular files must exactly match release.files (path, size, SHA-256). No source checkout or manifest inside the archive is required. Extraction is bounded by the release file sizes and the existing 16 MiB response bound.

## Transport
The existing timeout and 16 MiB streaming response bound apply to builds. Initial URLs are HTTPS or loopback HTTP. Builds permit at most five redirects, whose destinations must be HTTPS. Catalog requests still follow no redirects. All subprocesses remain inside transport; tar/flate2 perform extraction in process.

## State and local storage
Keep catalog as the verified protection marker. Add build provenance separately: build catalog location and the complete release record, including manifest, files, source, commit, path and archive descriptor. Revision state records enough provenance to restore previous metadata and installation source. Keep verified current and previous output under builds/<id>/ keyed by revision content. Both the state and stored output are committed by the existing transaction journal. No clone is created for a build install. Inactive output stays cached, so enable, sync-dist and rollback work offline. Retain only the current and previous versions; forget retains output for restore, uninstall removes it.

## Updates and Git transitions
Build installs follow their recorded catalog even when unverified. Verified protection remains unchanged. Catalog versions without build use Git. Explicit ref, branch, dev or detach switches to Git through a staged clone and keeps the former build available for offline rollback. Fork uses the recorded Git source and commit. All dependency resolution and atomicity guarantees apply equally to build candidates, including update all.

## Errors and atomicity
Unavailable artifacts fail with http, archive/hash/file mismatches with catalog, unsupported entry types with unsupported. Failures drop staging and preserve workspace state, deployed bytes and managed caches. Missing cached rollback output fails with no_previous_version. No build command or package code is executed.
