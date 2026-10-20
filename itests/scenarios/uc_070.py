"""UC-070: sync-dist repairs a workspace written by gitpm 0.4.

gitpm 0.4 deployed every installed package; 0.5 keeps only the active ones. A 0.4-style
workspace is reproduced from g2_demos_installed (A = vendor/demo, B = author/demo): A is
disabled and its dist/ folder is put back by hand, as 0.4 left it. Read-only commands
leave such a tree alone; `sync-dist --dry-run` lists the drift, `sync-dist` fixes it, and
the next mutating command would reconcile it anyway.
"""

import shutil

from gitpmtest import T, case

A, B = "vendor/demo", "author/demo"


def stale(t: T):
    """A disabled with a leftover dist/A of its installed revision (0.4 layout)."""
    t.gitpm_ok("disable", A)
    t.write(t.dist(A), {"index.js": f'export const greeting = "{A} 0.1.0";\n'})


@case(fixture="g2_demos_installed", spec=["dist/read-only-commands-never-change-dist#list-on-a-0-4-workspace"])
def list_leaves_a_stale_tree_alone(t: T):
    """list on a 0.4-style workspace changes nothing under dist/."""
    stale(t)
    before = t.snap(t.ws / "dist")
    t.gitpm_ok("list")
    t.same(before, "dist/ after list")


@case(fixture="g2_demos_installed", spec=["dist/sync-dist-repairs-dist-explicitly#dry-run"])
def dry_run_lists_drift_and_changes_nothing(t: T):
    """sync-dist --dry-run names the stale folder and the missing one; nothing changes."""
    stale(t)
    shutil.rmtree(t.dist(B))
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok("sync-dist", "--dry-run")
    t.eq(r.out, f"{B} would be deployed to dist/{B}\n{A} would be removed from dist/{A}\n"
                "Nothing was changed (--dry-run).\n", "output")
    t.same(before, "workspace")


@case(fixture="g2_demos_installed", spec=["dist/sync-dist-repairs-dist-explicitly#nothing-to-do"])
def sync_dist_fixes_then_has_nothing_to_do(t: T):
    """sync-dist removes the stale folder and deploys the missing one; a second run has nothing to do."""
    stale(t)
    shutil.rmtree(t.dist(B))
    r = t.gitpm_ok("sync-dist")
    t.eq(r.out, f"{B} deployed to dist/{B}\n{A} removed from dist/{A}\n", "output")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.eq((t.dist(B) / "index.js").read_text(), f'export const greeting = "{B} 0.1.0";\n', "dist/B deployed")
    before = t.snap(t.ws, git=True)
    t.eq(t.gitpm_ok("sync-dist").out, "dist/ already holds exactly the active packages.\n", "second run")
    t.same(before, "workspace after the second run")


@case(fixture="g2_demos_installed", spec=["dist/sync-dist-repairs-dist-explicitly#nothing-to-do",
                                          "dist/unknown-paths-are-left-alone"])
def unmanaged_paths_are_only_warned(t: T):
    """With dist/ matching the active packages, an unknown dist/custom is warned about, not counted."""
    t.write(t.ws / "dist/custom", {"keep.js": "custom\n"})
    r = t.gitpm_ok("sync-dist")
    t.eq(r.out, "dist/ already holds exactly the active packages.\n", "output")
    t.has(r.err, "Warning: dist/custom belongs to no known package or is not a folder; left as it is")
    t.ok((t.ws / "dist/custom/keep.js").is_file(), "dist/custom kept")


@case(fixture="g2_demos_installed", spec=["dist/every-mutating-command-reconciles-all-of-dist#stale-folder-from-gitpm-0-4"])
def next_mutation_reconciles_the_stale_tree(t: T):
    """Installing an unrelated package removes the disabled package's leftover dist in the same command."""
    stale(t)
    t.new_package("dan/other", "0.1.0")
    t.gitpm_ok("install", t.repo("dan/other"))
    t.ok(not t.dist(A).exists(), "stale dist/A removed")
    t.has((t.dist("dan/other") / "index.js").read_text(), "dan/other 0.1.0")
    t.has((t.dist(B) / "index.js").read_text(), f"{B} 0.1.0")


@case(fixture="g2_demos_installed", spec=["dist/sync-dist-repairs-dist-explicitly"])
def undeployable_package_is_named_and_others_synced(t: T):
    """B's clone and dist gone, A stale: sync-dist removes dist/A, then fails with not_deployed naming B
    and a hint."""
    stale(t)
    shutil.rmtree(t.clone(B))
    shutil.rmtree(t.dist(B))
    r = t.gitpm_err("sync-dist", kind="not_deployed", contains=f"Error: 1 package(s) not deployed: {B}")
    t.has(r.out, f"{A} removed from dist/{A}")
    t.has(r.err, f"Hint: `gitpm uninstall {B}`")
    # The hint works: uninstall drops the record whose clone is gone, then dist/ is in sync.
    t.gitpm_ok("uninstall", B)
    t.not_installed(B)
    t.gitpm_ok("sync-dist")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.ok(not t.dist(B).exists(), "dist/B not fabricated")


@case(fixture="g2_demos_installed", spec=["dist/unknown-paths-are-left-alone"])
def blocked_namespace_with_sync_dist(t: T):
    """An active package below a blocked namespace is reported while another is repaired."""
    blocked = t.dist(A).parent
    shutil.rmtree(blocked)
    blocked.write_text("blocked")
    shutil.rmtree(t.dist(B).parent)

    r = t.gitpm_err("sync-dist", kind="not_deployed", contains=f"Error: 1 package(s) not deployed: {A}")
    t.has(r.err, f"Warning: {A}: dist not deployed (dist/vendor is not a folder)")
    t.has(r.err, "Hint: remove or rename `dist/vendor` and run `gitpm sync-dist` again")
    t.ok(blocked.read_text() == "blocked", "blocked namespace file preserved")
    t.ok(t.dist(B).exists(), "unrelated active package deployed")

    blocked.unlink()
    t.gitpm_ok("sync-dist")
    t.ok(t.dist(A).exists(), "removing the obstruction allows deployment")
