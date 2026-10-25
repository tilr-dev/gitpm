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

from gitpmtest import R, T, case, gitpm_argv

ID = "3dce1568-569c-5b53-9321-37185e751947"
_gates = iter(range(10_000))


def race(t: T, jobs: list[list]) -> list[R]:
    gate = t.root / f".gate-{next(_gates)}"
    os.mkfifo(gate)
    procs = [subprocess.Popen(["sh", "-c", 'read _ < "$0"; exec gitpm "$@"', str(gate), *gitpm_argv(args)],
                              cwd=t.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                              stdin=subprocess.DEVNULL) for args in jobs]
    time.sleep(0.5)
    with open(gate, "w"):
        pass  # every waiting process starts now
    results = []
    for args, p in zip(jobs, procs):
        out, err = p.communicate(timeout=120)
        r = R(["gitpm", *gitpm_argv(args)], p.returncode, out, err)
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
    ids = [f"b19dabd5-0619-5414-b4d6-56c38472e88d{i}" for i in range(1, 7)]
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
    t.new_package("b21e58fb-a578-5d32-8d48-f420309785c9", "0.1.0")
    jobs = [["install", t.repo("b21e58fb-a578-5d32-8d48-f420309785c9")] for _ in range(4)]
    results = race(t, jobs)
    installed = [r for r in results if r.code == 0 and r.out.startswith("installed b21e58fb-a578-5d32-8d48-f420309785c9@0.1.0")]
    t.ok(len(installed) <= 1, "at most one process installed it")
    for args, r in zip(jobs, results):
        if r not in installed:
            t.ok("Error kind: locked" in r.err or (r.code == 0 and r.out == "b21e58fb-a578-5d32-8d48-f420309785c9 already installed (0.1.0)\n"),
                 "loser is locked or sees the package installed")
    if not installed and not any(r.code == 0 for r in results):
        t.gitpm_ok("install", t.repo("b21e58fb-a578-5d32-8d48-f420309785c9"))
    before = t.snap(t.ws, git=True)
    t.eq(t.gitpm_ok("install", t.repo("b21e58fb-a578-5d32-8d48-f420309785c9")).out, "b21e58fb-a578-5d32-8d48-f420309785c9 already installed (0.1.0)\n", "repeat install")
    t.same(before, "workspace after the repeat install")
    t.eq(t.package("b21e58fb-a578-5d32-8d48-f420309785c9")["current"]["version"], "0.1.0", "installed once")
    t.eq(len([p for p in t.state()["packages"] if p == "b21e58fb-a578-5d32-8d48-f420309785c9"]), 1, "one entry")
    clean_txn(t, t.ws)


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock#two-cli-processes"])
def same_workspace_mixed_mutations_lose_nothing(t: T):
    for id in ["807b9715-06b4-55c2-add2-59b487bfa42e", "03c25ec4-f583-513d-adfb-092fb98d8dba", "78d36dc2-2c10-5dc8-a889-056564906e06"]:
        t.new_package(id, "0.1.0")
    for id in ["807b9715-06b4-55c2-add2-59b487bfa42e", "03c25ec4-f583-513d-adfb-092fb98d8dba"]:
        t.gitpm_ok("install", t.repo(id))
    new = t.release(ID, "0.1.1")
    jobs = [["update", ID], ["disable", "807b9715-06b4-55c2-add2-59b487bfa42e"], ["install", t.repo("78d36dc2-2c10-5dc8-a889-056564906e06")], ["uninstall", "03c25ec4-f583-513d-adfb-092fb98d8dba", "--force"]]
    settle(t, jobs, race(t, jobs), "workspace is already in use")
    s = t.state()["packages"]
    t.eq(s[ID]["current"], {"version": "0.1.1", "commit": new}, "update kept")
    t.eq(s["807b9715-06b4-55c2-add2-59b487bfa42e"]["enabled"], False, "disable kept")
    t.ok("78d36dc2-2c10-5dc8-a889-056564906e06" in s and t.clone("78d36dc2-2c10-5dc8-a889-056564906e06").is_dir(), "install kept")
    t.ok("03c25ec4-f583-513d-adfb-092fb98d8dba" not in s and not t.clone("03c25ec4-f583-513d-adfb-092fb98d8dba").exists(), "uninstall kept")
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
        expected[("3dce1568-569c-5b53-9321-37185e751947", v)] = t.release(ID, v)
        jobs.append(["register", t.repo(ID), "--ref", f"v{v}"])
    for n in range(1, 4):
        id = f"521101f8-c190-541c-83b0-8fb50b51cf2a{n}"
        expected[(id, "0.1.0")] = t.new_package(id, "0.1.0")
        jobs.append(["register", t.repo(id), "--ref", f"v0.1.0"])
    settle(t, jobs, race(t, jobs), "catalog locked")
    index = json.loads(t.catalog_index(ID).read_text())
    t.eq(sorted(index["versions"]), sorted(["0.1.0", "0.1.1", "0.2.0"] + [f"0.3.{p}" for p in range(5)]),
         "no lost version in the shared package index")
    for (id, v), commit in expected.items():
        t.eq(json.loads(t.catalog_release(id, v).read_text())["commit"], commit, f"{id}@{v} recorded")
    t.eq(sorted(json.loads((t.catalog / "packages/dan/catalog.json").read_text())["directories"]),
         sorted(["hello", "n1", "n2", "n3", "num"]), "no lost directory link")
    for path, data in old.items():
        t.ok(path.read_bytes() == data, f"existing {path.relative_to(t.catalog)} unchanged")
    t.eq(list(t.catalog.rglob("*.tmp")), [], "no temporary files")
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "1e0e9f63-8c9a-5b31-a7bd-9358a4d189f2", "b538e2fa-c0de-5ac5-8358-e1b14bb3b0d0", "latest: 0.3.4")


@case(fixture="catalog", spec=["transactions-locking/workspace-lock", "catalog/writers-are-serialized-and-crash-safe"])
def independent_workspaces_and_catalogs_run_in_parallel(t: T):
    roots = [t.root / f"ws{i}" for i in range(1, 7)]
    jobs = [["--root", root, "install-catalog", ID, "--version", "0.1.1"] for root in roots]
    results = race(t, jobs)
    t.eq([r.code for r in results], [0] * len(jobs), "no refusal across different workspaces")
    for root in roots:
        state = json.loads((root / "state.json").read_text())
        t.eq(state["packages"][ID]["current"]["version"], "0.1.1", f"{root.name} state")
        t.has((root / "dist" / ID / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.1")
        clean_txn(t, root)
    catalogs = [t.root / f"cat{i}" for i in range(1, 5)]
    jobs = [["--catalog", c, "register", t.repo(ID), "--ref", f"v0.2.0"] for c in catalogs]
    results = race(t, jobs)
    t.eq([r.code for r in results], [0] * len(jobs), "no refusal across different catalogs")
    reference = t.catalog_release(ID, "0.2.0").read_text()
    for c in catalogs:
        t.eq(json.loads(t.catalog_release(ID, "0.2.0", c).read_text())["commit"], json.loads(reference)["commit"],
             f"{c.name} release")
