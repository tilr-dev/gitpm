"""gitpmtest: fixtures and scenario cases for the gitpm CLI, run inside Dagger containers.

Layout of every container (see src/gitpm_itests/main.py):

    /fx            the fixture snapshot; the case's working directory
    /fx/repos/ID   author repositories (git), e.g. /fx/repos/dan/hello
    /fx/ws         GITPM_ROOT (workspace)
    /fx/catalog    GITPM_CATALOG (local catalog; an empty folder until a case fills it)
    /fx/sources.txt  GITPM_SOURCES
    gitpm          the CLI under test, on PATH (gitpm-api: crates/gitpm/examples/api-parity.rs)

A fixture builds on its parents' merged /fx and must leave a valid state; its
check runs right after the build, so a broken fixture fails instead of its cases.
"""

from __future__ import annotations

import dataclasses
import hashlib
import http.server
import json
import os
import shlex
import subprocess
import threading
from pathlib import Path
from typing import Callable

FX = Path(os.environ.get("GITPMT_FX", "/fx"))  # GITPMT_FX: run outside a container
CLOCK_START = 1_760_000_000  # 2025-10-09; every git commit/tag gets the next second

FIXTURES: dict[str, "Fixture"] = {}
CASES: list["Case"] = []


# Catalog commands of the current gitpm CLI, by the name scenarios use. A rename of the
# CLI (change cli-catalog-commands: `catalog show|links|update|register`) is a
# change of this mapping only; T.gitpm rewrites the command word before running gitpm.
# `t.run("gitpm", ...)` bypasses the mapping (for cases about the exact spelling).
COMMANDS: dict[str, tuple[str, ...]] = {
    "info": ("catalog", "show"),
    "catalog": ("catalog", "links"),
    "catalog-update": ("catalog", "update"),
    "register": ("catalog", "register"),
}
# Global options taking a value (they may precede the command word).
VALUE_OPTIONS = {"--root", "--catalog", "--sources"}


def gitpm_argv(args) -> list[str]:
    """args with the command word mapped through COMMANDS."""
    args = [str(a) for a in args]
    i = 0
    while i < len(args):
        if args[i] in VALUE_OPTIONS:
            i += 2
            continue
        if args[i].startswith("-"):
            i += 1
            continue
        if args[i] == "catalog" and i + 1 < len(args) and args[i + 1] in {
            "show", "links", "update", "register", "--help", "-h"
        }:
            return args
        if args[i] in COMMANDS:
            return args[:i] + list(COMMANDS[args[i]]) + args[i + 1:]
        break
    return args


@dataclasses.dataclass
class Fixture:
    name: str
    module: str
    requires: list[str]
    gitpm: bool
    build: Callable
    check: Callable | None = None
    doc: str = ""


@dataclasses.dataclass
class Case:
    id: str
    module: str
    fixture: str | None
    fn: Callable
    title: str
    spec: list[str]
    xfail: str = ""


def fixture(*, requires: tuple[str, ...] | list[str] = (), gitpm: bool = False):
    """Register a fixture. gitpm=False: only git may be used (the binary is absent and the
    snapshot survives code changes). Decorate a check with @<fixture>.check."""
    def deco(fn):
        fx = Fixture(fn.__name__, fn.__module__, list(requires), gitpm, fn, doc=(fn.__doc__ or "").strip())
        if fx.name in FIXTURES:
            raise ValueError(f"duplicate fixture {fx.name}")
        FIXTURES[fx.name] = fx

        def check(chk):
            fx.check = chk
            return chk
        fn.check = check
        return fn
    return deco


def case(*, fixture: str | None = None, title: str = "", spec: list[str] | tuple[str, ...] = (),
         xfail: str = ""):
    """Register a scenario case; it starts from a fresh copy of the fixture snapshot.

    spec: tags CAPABILITY/REQUIREMENT-SLUG[#SCENARIO-SLUG] of openspec/specs (or
    "none:REASON"); a module-level SPEC list is used when the case names none.
    xfail: the change that fixes a known defect; the case is reported XFAIL while it
    fails and XPASS (a failure) once it passes."""
    def deco(fn):
        short = fn.__module__.rsplit(".", 1)[-1]
        CASES.append(Case(f"{short}:{fn.__name__}", fn.__module__, fixture, fn,
                          title or (fn.__doc__ or "").strip().splitlines()[0] if (title or fn.__doc__) else "",
                          list(spec), xfail))
        return fn
    return deco


class Blocked(Exception):
    """The scenario requires a capability absent from this runtime."""


class Failure(AssertionError):
    pass


@dataclasses.dataclass
class R:
    argv: list[str]
    code: int
    out: str
    err: str

    def json(self):
        return json.loads(self.out)

    @property
    def text(self) -> str:
        return self.out + self.err


def _indent(text: str, limit: int = 60) -> str:
    lines = text.rstrip("\n").splitlines()
    if len(lines) > limit:
        lines = lines[: limit // 2] + [f"... {len(lines) - limit} lines ..."] + lines[-limit // 2:]
    return "".join(f"    {l}\n" for l in lines)


class T:
    """Context of a fixture build or a case: commands, authoring helpers, assertions."""

    def __init__(self, root: Path = FX):
        self.root = root
        self.ws = root / "ws"
        self.catalog = root / "catalog"

    def catalog_empty(self) -> bool:
        """Nothing was written to /fx/catalog (it starts as an empty folder, see
        `__main__.ready_catalog`)."""
        return not self.catalog.exists() or not any(self.catalog.iterdir())

    # ---------------------------------------------------------------- processes

    def run(self, *argv, cwd=None, env=None, input: str | None = None, timeout: int = 120, quiet=False,
            check: bool = False) -> R:
        argv = [str(a) for a in argv]
        full_env = dict(os.environ, **(env or {}))
        p = subprocess.run(argv, cwd=cwd or self.root, env=full_env, input=input, capture_output=True,
                           text=True, timeout=timeout, stdin=None if input is not None else subprocess.DEVNULL)
        r = R(argv, p.returncode, p.stdout, p.stderr)
        if not quiet:
            print(f"$ {shlex.join(argv)}" + (f"   (cwd {cwd})" if cwd else ""))
            if r.out:
                print(_indent(r.out), end="")
            if r.err:
                print("  stderr:\n" + _indent(r.err), end="")
            print(f"  -> exit {r.code}")
        if check and r.code != 0:
            raise Failure(f"{shlex.join(argv)} exited {r.code}\n{r.text}")
        return r

    def gitpm(self, *args, **kw) -> R:
        """Run gitpm; the command word goes through COMMANDS."""
        self._remember_clones()
        return self.run("gitpm", *gitpm_argv(args), **kw)

    def gitpm_ok(self, *args, **kw) -> R:
        r = self.gitpm(*args, **kw)
        if r.code != 0:
            raise Failure(f"gitpm {shlex.join(map(str, args))} exited {r.code}\n{r.err}")
        return r

    def gitpm_err(self, *args, kind: str | None = None, contains: str | tuple[str, ...] = (), **kw) -> R:
        """Expect a non-zero exit; kind is matched against `Error kind: KIND`."""
        r = self.gitpm(*args, **kw)
        if r.code == 0:
            raise Failure(f"gitpm {shlex.join(map(str, args))} succeeded, expected a failure")
        if kind is not None:
            self.has(r.err, f"Error kind: {kind}")
        self.has(r.err, *([contains] if isinstance(contains, str) else contains))
        return r

    def _clock(self) -> dict[str, str]:
        f = self.root / ".gitpmt-clock"
        n = int(f.read_text()) + 1 if f.exists() else 0
        f.write_text(str(n))
        stamp = f"@{CLOCK_START + n} +0000"
        return {"GIT_AUTHOR_DATE": stamp, "GIT_COMMITTER_DATE": stamp}

    def git(self, repo: Path | str, *args, check: bool = True) -> str:
        repo = self.path(repo)
        env = self._clock() if args and args[0] in ("commit", "tag", "merge", "cherry-pick") else None
        r = self.run("git", "-C", repo, *args, env=env, quiet=True)
        if check and r.code != 0:
            raise Failure(f"git {shlex.join(map(str, args))} in {repo} failed ({r.code}):\n{r.text}")
        return r.out.strip()

    # ---------------------------------------------------------------- authoring

    def path(self, repo_or_id: Path | str) -> Path:
        """An author repository by ID (dan/hello) or any path."""
        s = str(repo_or_id)
        if s.startswith("/") or s.startswith("."):
            return Path(s)
        return self.root / "repos" / s

    def repo(self, id: str) -> Path:
        return self.root / "repos" / id

    def new_package(self, id: str, version: str = "0.1.0", **kw) -> str:
        """Create an author repository on branch main with its first tagged release."""
        repo = self.repo(id)
        (repo / "dist").mkdir(parents=True)
        self.git(repo, "init", "-q", "-b", "main")
        return self.release(id, version, **kw)

    def manifest(self, id: str, version: str, deps: dict | None = None, extra: dict | None = None) -> dict:
        m = {"id": id, "version": version}
        if deps is not None:
            m["dependencies"] = deps
        m.update(extra or {})
        return m

    def write(self, repo: Path | str, files: dict[str, str | bytes | None]):
        root = self.path(repo)
        for name, content in files.items():
            p = root / name
            if content is None:
                p.unlink()
                continue
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(content) if isinstance(content, bytes) else p.write_text(content)

    def release(self, id: str, version: str, deps: dict | None = None, extra: dict | None = None,
                files: dict | None = None, tag: bool = True, repo: Path | str | None = None) -> str:
        """Write plugin.json and dist/index.js ("ID VERSION"), commit, tag vVERSION; returns the commit."""
        repo = self.path(repo or id)
        self.write(repo, {"plugin.json": json.dumps(self.manifest(id, version, deps, extra), indent=2) + "\n",
                          "dist/index.js": f"export const greeting = {json.dumps(f'{id} {version}')};\n",
                          **(files or {})})
        self.git(repo, "add", "--all")
        self.git(repo, "commit", "-q", "-m", f"{id} {version}")
        if tag:
            self.git(repo, "tag", f"v{version}")
        return self.rev(repo)

    def commit(self, repo: Path | str, message: str, files: dict | None = None) -> str:
        """An untagged commit (optionally with file changes); returns its SHA."""
        if files:
            self.write(repo, files)
        self.git(repo, "add", "--all")
        self.git(repo, "commit", "-q", "--allow-empty", "-m", message)
        return self.rev(repo)

    def tag(self, repo: Path | str, name: str, rev: str = "HEAD", force: bool = False):
        self.git(repo, "tag", *(["-f"] if force else []), name, rev)

    def rev(self, repo: Path | str, ref: str = "HEAD") -> str:
        return self.git(repo, "rev-parse", f"{ref}^{{commit}}")

    # ---------------------------------------------------------------- workspace

    def state(self) -> dict:
        f = self.ws / "state.json"
        return json.loads(f.read_text()) if f.exists() else {}

    def package(self, id: str) -> dict:
        packages = self.state().get("packages", {})
        if id not in packages:
            raise Failure(f"{id} is not in state.json (have: {', '.join(packages) or 'none'})")
        return packages[id]

    def _remember_clones(self):
        if not hasattr(self, "_clone_keys"):
            self._clone_keys = {}
        try:
            state = self.state()
        except (ValueError, OSError):
            return
        for id, record in (state.get("packages", {}) | state.get("forgotten", {})).items():
            if record.get("clone"):
                self._clone_keys[id] = record["clone"]

    def worktree(self, id: str, root: Path | None = None) -> Path:
        """Detached checkout owned by this installed or forgotten package."""
        return (root or self.ws) / "worktrees" / id

    def clone(self, id: str, root: Path | None = None) -> Path:
        self._remember_clones()
        key = self._clone_keys.get(id)
        if root is not None and (root / "state.json").exists():
            state = json.loads((root / "state.json").read_text())
            record = (state.get("packages", {}) | state.get("forgotten", {})).get(id, {})
            key = record.get("clone", key)
        if key is None:
            source = str(self.repo(id).resolve())
            name = source.rstrip("/").replace(":", "/").rsplit("/", 1)[-1].removesuffix(".git")
            name = "".join(c for c in name if c.isascii() and (c.isalnum() or c in "-_"))[:40].lstrip("-")
            digest = hashlib.sha256(source.encode()).hexdigest()[:16]
            key = f"{name}-{digest}" if name else digest
        return (root or self.ws) / "repos" / key

    def dist(self, id: str) -> Path:
        return self.ws / "dist" / id

    def not_installed(self, id: str):
        """No state entry, clone, dist or leftover transaction for the ID."""
        self.ok(id not in self.state().get("packages", {}), f"{id} not in state")
        self.ok(not self.clone(id).exists(), f"no clone of {id}")
        self.ok(not self.worktree(id).exists(), f"no worktree of {id}")
        self.ok(not self.dist(id).exists(), f"no dist of {id}")
        leftovers = [str(p) for p in self.ws.rglob("*.pm-txn*")] if self.ws.exists() else []
        self.eq(leftovers, [], "leftover transactions")

    # ------------------------------------------------------------------ catalog

    def catalog_index(self, id: str, catalog: Path | None = None) -> Path:
        """namespaces/NS/NAME/plugin.json of a v2 catalog."""
        ns, name = id.split("/")
        return (catalog or self.catalog) / "namespaces" / ns / name / "plugin.json"

    def catalog_release(self, id: str, version: str, catalog: Path | None = None) -> Path:
        return self.catalog_index(id, catalog).parent / "versions" / version / "release.json"

    def edit_json(self, path: Path, change: Callable[[dict], None]):
        value = json.loads(path.read_text())
        change(value)
        path.write_text(json.dumps(value, indent=2) + "\n")
        print(f"  edited {path}")

    # ---------------------------------------------------------------- snapshots

    def snap(self, *paths: Path | str, git: bool = False) -> "Snap":
        """Content hashes of files and symlink targets under the paths (default: the workspace).
        Bare source metadata is omitted except package worktree HEADs unless git=True:
        fetch updates remote refs, FETCH_HEAD and objects without changing installations."""
        paths = tuple(Path(p) for p in paths) or (self.ws,)
        out = Snap(paths=paths, git=git)
        for base in paths:
            if not base.exists():
                out[str(base)] = "<absent>"
                continue
            for p in [base] if base.is_file() else sorted(base.rglob("*")):
                parts = p.relative_to(base).parts
                if not git and p.is_relative_to(self.ws / "repos"):
                    relative = p.relative_to(self.ws / "repos")
                    if len(relative.parts) < 4 or relative.parts[1] != "worktrees" or p.name != "HEAD":
                        continue
                if ".git" in parts and not git and parts[parts.index(".git") + 1:] != ("HEAD",):
                    continue
                if p.is_symlink():
                    out[str(p)] = "link:" + os.readlink(p)
                elif p.is_file():
                    out[str(p)] = hashlib.sha256(p.read_bytes()).hexdigest()
        return out

    def same(self, before: "Snap", what: str = "workspace"):
        """Assert nothing was added, removed or changed under the snapshot's paths."""
        after = self.snap(*before.paths, git=before.git)
        diff = sorted(set(before) ^ set(after) | {k for k in before if k in after and before[k] != after[k]})
        if diff:
            raise Failure(f"{what} changed: " + ", ".join(diff[:20]))
        print(f"  ok: {what} unchanged ({len(before)} files)")

    # ---------------------------------------------------------------- assertions

    def eq(self, actual, expected, what: str = "value"):
        if actual != expected:
            raise Failure(f"{what}: expected {expected!r}, got {actual!r}")
        print(f"  ok: {what} == {expected!r}")

    def ok(self, cond, what: str):
        if not cond:
            raise Failure(what)
        print(f"  ok: {what}")

    def has(self, text: str, *needles: str):
        for n in needles:
            if n not in text:
                raise Failure(f"expected {n!r} in:\n{text}")
            print(f"  ok: output contains {n!r}")

    def lacks(self, text: str, *needles: str):
        for n in needles:
            if n in text:
                raise Failure(f"unexpected {n!r} in:\n{text}")

    # ---------------------------------------------------------------- services

    def serve(self, directory: Path | str) -> "Server":
        """Serve a directory over loopback HTTP, logging every request path."""
        return Server(Path(directory))


class Snap(dict):
    def __init__(self, paths, git):
        super().__init__()
        self.paths, self.git = paths, git


class Server:
    def __init__(self, directory: Path):
        self.requests: list[str] = []
        log = self.requests

        class Handler(http.server.SimpleHTTPRequestHandler):
            def __init__(self, *a, **kw):
                super().__init__(*a, directory=str(directory), **kw)

            def log_message(self, *_):
                pass

            def do_GET(self):
                log.append(self.path)
                super().do_GET()

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def take(self) -> list[str]:
        """Requests since the last take."""
        out = list(self.requests)
        self.requests.clear()
        return out
