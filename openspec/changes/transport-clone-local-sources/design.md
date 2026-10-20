# Cause and reproduction

The failing operation used `git clone --no-hardlinks` on a local path.
That option disables hardlinks, but retains Git's local directory-copy mode.
`builtin/clone.c:copy_or_link_directory` walks all of `objects`, including
server-info metadata. `copy.c:copy_file_with_time` opens and copies each file,
then stats its original pathname to preserve timestamps. A pathname removed
between enumeration and open, or before that final stat, produces ENOENT and
`failed to copy file to ...`, aborting an otherwise valid source clone.

Git's `server-info.c:update_info_file` creates `packs_XXXXXX` beside
`objects/info/packs`, then renames it into place or deletes it. Such a file
is metadata for dumb HTTP, not a committed object. The observed TILR-53 error
names exactly this kind of file. The old log does not identify which process
updated server-info; we do not claim to have recovered that process.

The regression uses an external filesystem primitive, a FIFO named
`objects/info/packs_controlled`. Its writer blocks until a copier opens it,
removes the pathname, then closes the stream. Git finishes copying but its
timestamp stat sees ENOENT. The control asserts the original diagnostic from
real `git clone --no-hardlinks`. Without the fix, registration then fails with
the same diagnostic (logs/transient-before.log in the task folder).

Transport cloning (`--no-local`) requests reachable Git objects and ignores
server-info files. It provides independent objects without changing the source
or disabling its maintenance. The harness releases an unused writer after the
operation returns, so the transport case also terminates without timeouts.
The regression covers all three source clone sites: register, update_sources,
and installation; it also checks origin and reading committed content after
removing the author repository. The existing dependency test checks edges.

## Evidence and recheck

On 2026-10-20, Git 2.54.0 on macOS reproduced the ENOENT control and the
unfixed registration failure. Recheck with:

```
git --version
cargo test -p gitpm --test dependencies local_source_clones_ignore_transient_server_info --locked
```

This is a choice of Git transport, not a version-specific workaround. The
control in that command rechecks the raw-copy behavior each time; if upstream
changes it, review the control and whether transport cloning is still needed.

Upstream implementation references:
- https://github.com/git/git/blob/v2.54.0/builtin/clone.c
- https://github.com/git/git/blob/v2.54.0/copy.c
- https://github.com/git/git/blob/v2.54.0/server-info.c
