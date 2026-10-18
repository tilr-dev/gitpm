"""UC-022: Uninstall, exact ID and namespace guards (P0).

Workspace (g3_trio_installed): dan/a, dan/b in one namespace and other/c in another.
The container file system is case-sensitive (ext4/overlayfs), so the `NS/P` subcase runs
there as an exact-ID check; the macOS case-insensitive variant needs a case-insensitive
mount, which a non-privileged container cannot create (BLOCKED case below).
"""

import shlex
from pathlib import Path

from gitpmtest import T, Failure, Blocked, case

A, B, C = "dan/a", "dan/b", "other/c"


def tty(t: T, args: list, answer: str = "") -> "object":
    """Run gitpm with stdin on a pseudo-terminal (util-linux script); `answer` is typed in."""
    return t.run("script", "-qec", shlex.join(["gitpm", *map(str, args)]), "/dev/null", input=answer)


def survivors(t: T, *ids: str):
    """Snapshot of the given packages' clones and dist folders plus all author repos."""
    paths = [t.clone(i) for i in ids] + [t.dist(i) for i in ids] + [t.root / "repos"]
    return t.snap(*paths, git=True)


@case(fixture="g3_trio_installed", spec=["install-uninstall/uninstall-is-planned-and-confirmed", "install-uninstall/only-manager-owned-files-are-deleted"])
def uninstall_clean_removes_only_selected(t: T):
    """1. uninstall of a clean dan/b on a TTY: no prompt; only its state, clone and dist go."""
    keep = survivors(t, A, C)
    state = t.state()
    r = tty(t, ["uninstall", B])
    t.eq(r.code, 0, "exit code")
    t.lacks(r.out, "Delete anyway", "Local work")
    t.not_installed(B)
    after = t.state()
    del state["packages"][B]
    t.eq(after, state, "state minus dan/b")
    t.same(keep, "siblings and author repositories")
    t.ok((t.ws / "packages/dan").is_dir(), "namespace directory of the sibling kept")


@case(fixture="g3_trio_installed", spec=["package-ids/package-id-syntax", "errors/failing-operations-leave-the-workspace-unchanged"])
def namespace_directory_is_refused(t: T):
    """2. uninstall of the namespace `dan`, without and with --force, is rejected as an
    invalid ID; siblings, author repositories and the catalog survive."""
    t.gitpm_ok("register", t.repo(A), "--ref", "dan/a@0.1.1")
    before = t.snap(t.ws, t.root / "repos", t.catalog, git=True)
    for extra in [[], ["--force"]]:
        t.gitpm_err("uninstall", "dan", *extra, kind="invalid_id", contains="namespace/name")
        t.same(before, f"workspace, author repos, catalog after uninstall dan {' '.join(extra)}")
    for id in [A, B]:
        t.package(id)


@case(fixture="g3_trio_installed", spec=["cli/errors-and-exit-codes#unknown-id", "errors/stable-error-kinds#removal-of-an-unknown-package"])
def unknown_id_changes_nothing(t: T):
    """3. unknown ID: non-zero exit naming the ID; nothing changes."""
    before = t.snap(t.ws, t.root / "repos", git=True)
    for extra in [[], ["--force"]]:
        r = t.gitpm_err("uninstall", "dan/zzz", *extra, contains="dan/zzz")
        t.has(r.err, "Error kind:")
        t.same(before, "workspace and author repos")
    t.ok(not (t.root / "catalog").exists(), "no catalog created")


@case(fixture="g3_trio_installed", spec=["package-ids/ids-are-matched-exactly"])
def case_variant_is_an_unknown_id(t: T):
    """4. `DAN/A` when only dan/a is installed: unknown ID with and without --force; dan/a
    clone, dist and state entry byte-identical (exact state ID only)."""
    before = t.snap(t.ws, git=True)
    entry = t.package(A)
    for extra in [[], ["--force"]]:
        r = t.gitpm_err("uninstall", "DAN/A", *extra, contains="DAN/A")
        t.has(r.err, "Error kind:")
        t.same(before, f"workspace after uninstall DAN/A {' '.join(extra)}")
    t.eq(t.package(A), entry, "dan/a state entry")


@case(fixture="g3_trio_installed", spec=["package-ids/ids-are-matched-exactly#case-variant-on-apfs"])
def case_variant_on_case_insensitive_fs(t: T):
    """4b. The same on a case-insensitive file system (macOS default)."""
    probe = t.root / "CaseProbe"
    probe.mkdir()
    insensitive = (t.root / "caseprobe").exists()
    probe.rmdir()
    if not insensitive:
        raise Blocked("requires a case-insensitive filesystem; run this case on macOS/APFS")
    case_variant_is_an_unknown_id(t)


@case(fixture="g3_trio_installed", spec=["install-uninstall/only-manager-owned-files-are-deleted#uninstalling-a-fork"])
def fork_author_repo_outside_workspace_survives(t: T):
    """5. uninstall touches only packages/<id> and dist/<id>: the fork author repository
    (outside the workspace) survives uninstall of the fork and of its original."""
    fork = t.root / "authors/my-a"
    t.gitpm_ok("fork", A, "--ns", "mine", "--into", fork)
    t.gitpm_ok("install", fork, "--channel", "dev")
    t.package("mine/a")
    keep = t.snap(fork, t.root / "repos", t.clone(B), t.dist(B), t.clone(C), t.dist(C), git=True)
    t.gitpm_ok("uninstall", "mine/a")
    t.not_installed("mine/a")
    t.ok(not (t.ws / "packages/mine").exists() or not any((t.ws / "packages/mine").iterdir()),
         "nothing left of mine/a in packages/")
    t.gitpm_ok("uninstall", A)
    t.not_installed(A)
    t.same(keep, "fork author repo, author repos and unrelated packages")
    t.eq(sorted(t.state()["packages"]), [B, C], "remaining packages")
    t.ok(Path(fork, "plugin.json").is_file(), "fork checkout intact")
