"""UC-050: Real concurrent CLI mutations (P0).

Independent `gitpm` processes are released at the same instant by a FIFO gate (each waits
in `read < gate`, then execs gitpm) and compete for the same workspace or catalog. The
contract: each mutation either succeeds or is refused with `Error kind: locked`
(workspace is already in use / catalog locked); refused ones succeed on retry; no
success is lost and unrelated package bytes never change.
"""

import json
import os
import subprocess
import time
from pathlib import Path

from gitpmtest import R, T, case

ID = "dan/hello"
_gates = iter(range(10_000))


def race(t: T, jobs: list[list]) -> list[R]:
    gate = t.root / f".gate-{next(_gates)}"
    os.mkfifo(gate)
    procs = [subprocess.Popen(["sh", "-c", 'read _ < "$0"; exec gitpm "$@"', str(gate), *map(str, args)],
                              cwd=t.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                              stdin=subprocess.DEVNULL) for args in jobs]
    time.sleep(0.5)
    with open(gate, "w"):
        pass  # every waiting process starts now
    results = []
    for args, p in zip(jobs, procs):
        out, err = p.communicate(timeout=120)
        r = R(["gitpm", *map(str, args)], p.returncode, out, err)
        print(f"$ {' '.join(r.argv)}  (concurrent)\n" + "".join(f"    {l}\n" for l in (out + err).strip().splitlines())
              + f"  -> exit {r.code}")
        results.append(r)
    gate.unlink()
    return results


def settle(t: T, jobs: list[list], results: list[R], busy: str, allowed: tuple[str, ...] = ()) -> list[list]:
    """Every result is a success or a clean lock refusal; retry the refused ones."""
    refused = []
    for args, r in zip(jobs, results):
        if r.code == 0:
            continue
        if "Error kind: locked" in r.err:
            t.has(r.err, busy)
            refused.append(args)
        elif not any(f"Error kind: {k}" in r.err for k in allowed):
            raise AssertionError(f"unexpected failure of gitpm {' '.join(map(str, args))}:\n{r.err}")
    print(f"  {len(jobs) - len(refused)} ran, {len(refused)} refused as locked")
    for args in refused:
        t.gitpm_ok(*args)
    return refused


def clean_txn(t: T, root: Path):
    t.ok(not (root / ".pm-txn").exists(), f"no transaction left in {root}")


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock#two-cli-processes"])
def same_workspace_concurrent_installs(t: T):
    ids = [f"dan/p{i}" for i in range(1, 7)]
    commits = {id: t.new_package(id, "0.1.0") for id in ids}
    hello = t.snap(t.clone(ID), t.dist(ID))
    hello_entry = t.package(ID)
    jobs = [["install", t.repo(id)] for id in ids]
    settle(t, jobs, race(t, jobs), "workspace is already in use")
    t.eq(sorted(t.state()["packages"]), sorted(ids + [ID]), "every install landed")
    for id in ids:
        t.eq(t.package(id)["current"], {"version": "0.1.0", "commit": commits[id]}, f"{id} state")
        t.eq(t.rev(t.clone(id)), commits[id], f"{id} clone HEAD")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} 0.1.0")
    t.same(hello, "unrelated package bytes")
    t.eq(t.package(ID), hello_entry, "unrelated package entry")
    clean_txn(t, t.ws)


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock", "install-uninstall/install-is-idempotent-for-the-same-source#repeat-install"])
def same_plugin_installed_concurrently_once(t: T):
    """Four processes install the same ID: at most one installs it, the rest are locked or
    report it already installed (idempotent install, exit 0)."""
    t.new_package("dan/solo", "0.1.0")
    jobs = [["install", t.repo("dan/solo")] for _ in range(4)]
    results = race(t, jobs)
    installed = [r for r in results if r.code == 0 and r.out.startswith("installed dan/solo@0.1.0")]
    t.ok(len(installed) <= 1, "at most one process installed it")
    for args, r in zip(jobs, results):
        if r not in installed:
            t.ok("Error kind: locked" in r.err or (r.code == 0 and r.out == "dan/solo already installed (0.1.0)\n"),
                 "loser is locked or sees the package installed")
    if not installed and not any(r.code == 0 for r in results):
        t.gitpm_ok("install", t.repo("dan/solo"))
    before = t.snap(t.ws, git=True)
    t.eq(t.gitpm_ok("install", t.repo("dan/solo")).out, "dan/solo already installed (0.1.0)\n", "repeat install")
    t.same(before, "workspace after the repeat install")
    t.eq(t.package("dan/solo")["current"]["version"], "0.1.0", "installed once")
    t.eq(len([p for p in t.state()["packages"] if p == "dan/solo"]), 1, "one entry")
    clean_txn(t, t.ws)


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock#two-cli-processes"])
def same_workspace_mixed_mutations_lose_nothing(t: T):
    for id in ["dan/a", "dan/b", "dan/c"]:
        t.new_package(id, "0.1.0")
    for id in ["dan/a", "dan/b"]:
        t.gitpm_ok("install", t.repo(id))
    new = t.release(ID, "0.1.1")
    jobs = [["update", ID], ["disable", "dan/a"], ["install", t.repo("dan/c")], ["uninstall", "dan/b", "--force"]]
    settle(t, jobs, race(t, jobs), "workspace is already in use")
    s = t.state()["packages"]
    t.eq(s[ID]["current"], {"version": "0.1.1", "commit": new}, "update kept")
    t.eq(s["dan/a"]["enabled"], False, "disable kept")
    t.ok("dan/c" in s and t.clone("dan/c").is_dir(), "install kept")
    t.ok("dan/b" not in s and not t.clone("dan/b").exists(), "uninstall kept")
    t.has((t.dist(ID) / "index.js").read_text(), "0.1.1")
    # Rounds of concurrent toggles over every package.
    ids = sorted(s)
    for enabled in [False, True, False]:
        jobs = [["enable" if enabled else "disable", id] for id in ids]
        settle(t, jobs, race(t, jobs), "workspace is already in use")
        t.eq({id: p["enabled"] for id, p in t.state()["packages"].items()}, {id: enabled for id in ids},
             f"all {'enabled' if enabled else 'disabled'}")
    clean_txn(t, t.ws)


@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe"])
def same_catalog_concurrent_registrations(t: T):
    old = {p: p.read_bytes() for p in t.catalog.rglob("release.json")}
    jobs, expected = [], {}
    for patch in range(5):
        v = f"0.3.{patch}"
        expected[("dan/hello", v)] = t.release(ID, v)
        jobs.append(["register", t.repo(ID), "--ref", f"{ID}@{v}"])
    for n in range(1, 4):
        id = f"dan/n{n}"
        expected[(id, "0.1.0")] = t.new_package(id, "0.1.0")
        jobs.append(["register", t.repo(id), "--ref", f"{id}@0.1.0"])
    settle(t, jobs, race(t, jobs), "catalog locked")
    index = json.loads(t.catalog_index(ID).read_text())
    t.eq(sorted(index["versions"]), sorted(["0.1.0", "0.1.1", "0.2.0"] + [f"0.3.{p}" for p in range(5)]),
         "no lost version in the shared package index")
    for (id, v), commit in expected.items():
        t.eq(json.loads(t.catalog_release(id, v).read_text())["commit"], commit, f"{id}@{v} recorded")
    t.eq(sorted(json.loads((t.catalog / "namespaces/dan/catalog.json").read_text())["directories"]),
         ["hello", "n1", "n2", "n3", "num"], "no lost directory link")
    for path, data in old.items():
        t.ok(path.read_bytes() == data, f"existing {path.relative_to(t.catalog)} unchanged")
    t.eq(list(t.catalog.rglob("*.tmp")), [], "no temporary files")
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "dan/n1", "dan/n3", "latest: 0.3.4")


@case(fixture="catalog", spec=["transactions-locking/workspace-lock", "catalog/writers-are-serialized-and-crash-safe"])
def independent_workspaces_and_catalogs_run_in_parallel(t: T):
    roots = [t.root / f"ws{i}" for i in range(1, 7)]
    jobs = [["--root", root, "install-catalog", ID, "--version", "0.1.1"] for root in roots]
    results = race(t, jobs)
    t.eq([r.code for r in results], [0] * len(jobs), "no refusal across different workspaces")
    for root in roots:
        state = json.loads((root / "state.json").read_text())
        t.eq(state["packages"][ID]["current"]["version"], "0.1.1", f"{root.name} state")
        t.has((root / "dist" / ID / "index.js").read_text(), "dan/hello 0.1.1")
        clean_txn(t, root)
    catalogs = [t.root / f"cat{i}" for i in range(1, 5)]
    jobs = [["--catalog", c, "register", t.repo(ID), "--ref", f"{ID}@0.2.0"] for c in catalogs]
    results = race(t, jobs)
    t.eq([r.code for r in results], [0] * len(jobs), "no refusal across different catalogs")
    reference = t.catalog_release(ID, "0.2.0").read_text()
    for c in catalogs:
        t.eq(json.loads(t.catalog_release(ID, "0.2.0", c).read_text())["commit"], json.loads(reference)["commit"],
             f"{c.name} release")
