"""Black-box integration tests of gitpm, run natively in Dagger.

Every test runs in its own container, concurrently:

* rust     — each libtest test of the gitpm workspace (unit and integration);
* lint     — clippy of the workspace;
* fixture  — building a prepared state (a cached snapshot) and checking its invariants;
* scenario — each case of itests/scenarios/uc_*.py, started from its fixture snapshot,
             driving only the `gitpm` binary (and `gitpm-api`, examples/api-parity.rs).

Fixtures are cached Dagger layers keyed by their inputs: a source-tier fixture (git only)
depends on its own module and the gitpmtest helper; a gitpm-tier fixture also depends on
the gitpm binary, so it is rebuilt once per code change, not once per case.
"""

import asyncio
import dataclasses
import datetime
import json
import uuid
from typing import Annotated

import dagger
from dagger import DefaultPath, Doc, Ignore, dag, function, object_type

RUST_IMAGE = "rust:1.94.0-bookworm"
RUNTIME_IMAGE = "debian:bookworm-slim"
APT = ["git", "python3", "ca-certificates", "curl", "bsdutils", "procps"]

# Same paths in every container: test binaries bake CARGO_MANIFEST_DIR and
# CARGO_BIN_EXE_gitpm (/target/debug/gitpm) at compile time.
SRC = "/src/gitpm"
SPECS = "/specs"
TARGET = "/target"
GITPM_BIN = f"{TARGET}/debug/gitpm"

# The repository root (the module's Git context), reduced to what the build and the
# tag validation read. The build itself gets the tree without openspec/ (see _build_src).
SOURCE_IGNORE = [
    "*",
    "!Cargo.toml", "!Cargo.lock", "!README.md",
    "!crates/", "crates/*/target/",
    "!tests/", "!.cargo/",
    "!openspec/", "openspec/*", "!openspec/specs/",
]
ITESTS_IGNORE = ["sdk/", ".venv/", "results/", "**/__pycache__/", "src/", "uv.lock"]

GITCONFIG = """[user]
\tname = gitpm Test
\temail = gitpm-test@example.invalid
[init]
\tdefaultBranch = main
[commit]
\tgpgsign = false
[tag]
\tgpgsign = false
[advice]
\tdetachedHead = false
"""

# Runs one test command and writes a single JSON result, so the module reads one file
# per test. The test exit code never fails the exec: a red test is data, not an error.
RUNNER = r'''#!/usr/bin/env python3
import json, os, subprocess, sys, time
timeout = int(os.environ.get("GITPMT_TIMEOUT", "600"))
start = time.monotonic()
try:
    p = subprocess.run(sys.argv[1:], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
    code, log = p.returncode, p.stdout
except subprocess.TimeoutExpired as e:
    code, log = 124, (e.stdout or b"") + f"\n[gitpmt] timeout after {timeout}s\n".encode()
ms = int((time.monotonic() - start) * 1000)
log = log.decode("utf-8", "replace")
if len(log) > 400_000:
    log = log[:200_000] + "\n[gitpmt] ... log truncated ...\n" + log[-200_000:]
with open("/gitpmt-result.json", "w") as f:
    json.dump({"code": code, "ms": ms, "log": log}, f)
'''

# Compiles every test target, the CLI and the api-parity example once, copies the
# executables out of the cache volume into the layer and records the test names of each.
BUILD = r'''#!/usr/bin/env python3
import json, os, shutil, subprocess
out = "/out"
os.makedirs(f"{out}/tests")
os.makedirs(f"{out}/bin")
suites = []
p = subprocess.run(["cargo", "test", "--no-run", "--locked", "--workspace",
                    "--message-format=json-render-diagnostics"],
                   stdout=subprocess.PIPE, check=True, text=True, cwd="/src/gitpm")
for line in p.stdout.splitlines():
    if not line.startswith("{"):
        continue
    m = json.loads(line)
    if m.get("reason") != "compiler-artifact" or not m.get("executable") or not m["profile"]["test"]:
        continue
    package = os.path.basename(os.path.dirname(m["manifest_path"]))
    kind = m["target"]["kind"][0]
    suite = f"{package}/{kind}" if kind in ("lib", "bin", "staticlib", "rlib", "cdylib") else f"{package}/{m['target']['name']}"
    exe = f"{out}/tests/{suite.replace('/', '__')}"
    shutil.copy2(m["executable"], exe)
    listed = subprocess.run([exe, "--list", "--format", "terse"], stdout=subprocess.PIPE, check=True, text=True)
    names = [l[: -len(": test")] for l in listed.stdout.splitlines() if l.endswith(": test")]
    suites.append({"suite": suite, "exe": exe, "manifest_dir": os.path.dirname(m["manifest_path"]), "tests": names})
subprocess.run(["cargo", "build", "--locked", "-p", "gitpm-cli", "--bin", "gitpm"], check=True, cwd="/src/gitpm")
shutil.copy2("/target/debug/gitpm", f"{out}/bin/gitpm")
subprocess.run(["cargo", "build", "--locked", "-p", "gitpm", "--example", "api-parity"], check=True, cwd="/src/gitpm")
shutil.copy2("/target/debug/examples/api-parity", f"{out}/bin/gitpm-api")
with open(f"{out}/manifest.json", "w") as f:
    json.dump(suites, f, indent=1)
'''


@dataclasses.dataclass
class Result:
    id: str
    kind: str
    status: str  # PASS | FAIL | BLOCKED | XFAIL | XPASS
    ms: int = 0
    log: str = ""
    spec: list[str] = dataclasses.field(default_factory=list)
    xfail: str = ""


FAILING = ("FAIL", "XPASS")


@object_type
class GitpmItests:
    source: Annotated[dagger.Directory, Doc("gitpm repository root"), DefaultPath("/"), Ignore(SOURCE_IGNORE)]
    itests: Annotated[dagger.Directory, Doc("gitpmtest helper, fixtures and scenarios"), DefaultPath("/itests"),
                      Ignore(ITESTS_IGNORE)]
    commit: Annotated[str, Doc("label of the tested revision (report only)")] = "worktree"

    # ---------------------------------------------------------------- containers

    def _apt(self, image: str) -> dagger.Container:
        return (
            dag.container()
            .from_(image)
            .with_exec(["sh", "-c", "apt-get update -qq && DEBIAN_FRONTEND=noninteractive "
                        f"apt-get install -y -qq --no-install-recommends {' '.join(APT)} >/dev/null"])
            .with_new_file("/etc/gitpmt-gitconfig", GITCONFIG)
            .with_env_variable("GIT_CONFIG_GLOBAL", "/etc/gitpmt-gitconfig")
            .with_env_variable("GIT_CONFIG_NOSYSTEM", "1")
            .with_env_variable("GIT_TERMINAL_PROMPT", "0")
            .with_env_variable("LANG", "C.UTF-8")
        )

    def _build_src(self) -> dagger.Directory:
        """The Cargo workspace without openspec/, so spec edits do not invalidate the build."""
        return self.source.without_directory("openspec")

    @function
    def runtime(self) -> dagger.Container:
        """Slim test runtime: git, python3, curl and the result runner."""
        return self._apt(RUNTIME_IMAGE).with_new_file("/usr/local/bin/gitpmt-run", RUNNER, permissions=0o755)

    def _rust(self, target_cache: str) -> dagger.Container:
        return (
            self._apt(RUST_IMAGE).with_exec(["rustup", "component", "add", "clippy"])
            .with_directory(SRC, self._build_src())
            # The whole CARGO_HOME, so cargo's package-cache lock is shared by concurrent containers.
            .with_mounted_cache("/cargo-home", dag.cache_volume("gitpm-itests-cargo-home"))
            .with_env_variable("CARGO_HOME", "/cargo-home")
            .with_mounted_cache(TARGET, dag.cache_volume(target_cache))
            .with_env_variable("CARGO_TARGET_DIR", TARGET)
            .with_env_variable("CARGO_TERM_COLOR", "never")
            # Bounded parallelism: the build and clippy share the engine (24 GiB on pb).
            .with_env_variable("CARGO_BUILD_JOBS", "6")
            .with_workdir(SRC)
        )

    @function
    def build(self) -> dagger.Directory:
        """Compiled test executables, gitpm, gitpm-api and manifest.json (suites and test names)."""
        return (
            self._rust("gitpm-itests-target-test")
            .with_new_file("/usr/local/bin/gitpmt-build", BUILD, permissions=0o755)
            .with_exec(["gitpmt-build"])
            .directory("/out")
        )

    @function
    def gitpm(self) -> dagger.File:
        """The gitpm CLI binary (Linux) built from the working tree."""
        return self.build().file("bin/gitpm")

    def _with_gitpm(self, ctr: dagger.Container) -> dagger.Container:
        return (ctr.with_file("/usr/local/bin/gitpm", self.gitpm())
                .with_file("/usr/local/bin/gitpm-api", self.build().file("bin/gitpm-api")))

    async def _run(self, sem: asyncio.Semaphore, ctr: dagger.Container, argv: list[str],
                   rid: str, kind: str, spec: list[str] | None = None, xfail: str = "") -> Result:
        async with sem:
            try:
                raw = await ctr.with_exec(["gitpmt-run", *argv]).file("/gitpmt-result.json").contents()
            except dagger.ExecError as e:
                return Result(rid, kind, "FAIL", 0, f"[gitpmt] runner failed:\n{e.stdout}\n{e.stderr}", spec or [], xfail)
            except dagger.QueryError as e:
                return Result(rid, kind, "FAIL", 0, f"[gitpmt] {e}", spec or [], xfail)
        r = json.loads(raw)
        if kind == "scenario" and r["code"] == 77:
            status = "BLOCKED"
        elif xfail:
            status = "XPASS" if r["code"] == 0 else "XFAIL"
        else:
            status = "PASS" if r["code"] == 0 else "FAIL"
        log = r["log"]
        if status == "XPASS":
            log += f"\n[gitpmt] XPASS: expected to fail until change {xfail}; remove the xfail marker\n"
        return Result(rid, kind, status, r["ms"], log, spec or [], xfail)

    # ------------------------------------------------------------- rust, lint

    async def _rust_tests(self, sem, run_id: str, filter: str) -> list[Result]:
        out = self.build()
        suites = json.loads(await out.file("manifest.json").contents())
        base = (
            self.runtime()
            .with_directory(SRC, self._build_src())
            .with_directory("/out/tests", out.directory("tests"))
            .with_file(GITPM_BIN, out.file("bin/gitpm"))
            # As .cargo/config.toml does for `cargo test`.
            .with_env_variable("GIT_CONFIG_GLOBAL", f"{SRC}/tests/gitconfig")
            .with_env_variable("GITPMT_RUN", run_id)
        )
        jobs = []
        for s in suites:
            for name in s["tests"]:
                rid = f"rust:{s['suite']}::{name}"
                if not _match(filter, rid):
                    continue
                ctr = base.with_workdir(s["manifest_dir"])
                jobs.append(self._run(sem, ctr, [s["exe"], "--exact", name, "--test-threads=1"], rid, "rust"))
        return list(await asyncio.gather(*jobs))

    async def _lint(self, sem, run_id: str, filter: str) -> list[Result]:
        rid = "lint:clippy"
        if not _match(filter, rid):
            return []
        ctr = (self._rust("gitpm-itests-target-clippy")
               .with_new_file("/usr/local/bin/gitpmt-run", RUNNER, permissions=0o755)
               .with_env_variable("GITPMT_RUN", run_id))
        return [await self._run(sem, ctr, ["cargo", "clippy", "--locked", "--workspace", "--all-targets",
                                           "--", "-D", "warnings"], rid, "lint")]

    # ---------------------------------------------------------------- fixtures

    def _gitpmtest(self, ctr: dagger.Container, *modules: str) -> dagger.Container:
        """Mount the helper and only the given modules: unrelated edits keep the caches."""
        ctr = ctr.with_directory("/itests/gitpmtest", self.itests.directory("gitpmtest"))
        for m in modules:
            path = m.replace(".", "/") + ".py"
            ctr = ctr.with_file(f"/itests/{path}", self.itests.file(path))
        return (ctr.with_env_variable("PYTHONPATH", "/itests")
                .with_env_variable("PYTHONDONTWRITEBYTECODE", "1")
                .with_env_variable("GITPM_ROOT", "/fx/ws")
                .with_env_variable("GITPM_CATALOG", "/fx/catalog")
                .with_env_variable("GITPM_SOURCES", "/fx/sources.txt")
                .with_env_variable("HOME", "/root")
                .with_workdir("/fx"))

    async def _registry(self) -> dict:
        ctr = (self.runtime().with_directory("/itests", self.itests)
               .with_directory(SPECS, self.source.directory("openspec/specs"))
               .with_env_variable("GITPMT_SPECS", SPECS)
               .with_env_variable("PYTHONPATH", "/itests")
               .with_env_variable("PYTHONDONTWRITEBYTECODE", "1"))
        return json.loads(await ctr.with_exec(["python3", "-m", "gitpmtest", "list"]).stdout())

    def _fixture_builder(self, registry: dict):
        """name -> Task[Directory]; each fixture is built once per test run."""
        tasks: dict[str, asyncio.Task] = {}
        fixtures = registry["fixtures"]

        async def build(name: str) -> dagger.Directory:
            spec = fixtures[name]
            fx = dag.directory()
            for parent in spec["requires"]:
                fx = fx.with_directory(".", await get(parent))
            ctr = self._gitpmtest(self.runtime(), spec["module"])
            if spec["gitpm"]:
                ctr = self._with_gitpm(ctr)
            ctr = ctr.with_directory("/fx", fx).with_exec(
                ["python3", "-m", "gitpmtest", "fixture", f"{spec['module']}:{name}"])
            await ctr.sync()
            return ctr.directory("/fx")

        def get(name: str) -> asyncio.Task:
            if name not in tasks:
                tasks[name] = asyncio.ensure_future(build(name))
            return tasks[name]

        return get

    async def _fixture_results(self, get, registry: dict, names) -> list[Result]:
        out = []
        for name in names:
            try:
                await get(name)
                out.append(Result(f"fixture:{name}", "fixture", "PASS"))
            except dagger.ExecError as e:
                failed_parent = any(_failed(get, p) for p in registry["fixtures"][name]["requires"])
                if failed_parent:
                    out.append(Result(f"fixture:{name}", "fixture", "BLOCKED", 0, "a required fixture failed"))
                else:
                    out.append(Result(f"fixture:{name}", "fixture", "FAIL", 0, f"{e.stdout}\n{e.stderr}"))
            except Exception as e:
                out.append(Result(f"fixture:{name}", "fixture", "FAIL", 0, _error_text(e)))
        return out

    # --------------------------------------------------------------- scenarios

    async def _scenarios(self, sem, run_id: str, filter: str, registry: dict) -> list[Result]:
        get = self._fixture_builder(registry)
        cases = [c for c in registry["cases"] if _match(filter, f"scenario:{c['id']}")]
        needed: set[str] = set()

        def closure(n):
            if n and n not in needed:
                needed.add(n)
                for p in registry["fixtures"][n]["requires"]:
                    closure(p)

        for c in cases:
            closure(c["fixture"])

        async def one(c) -> Result:
            rid = f"scenario:{c['id']}"
            fx = dag.directory()
            if c["fixture"]:
                try:
                    fx = await get(c["fixture"])
                except Exception:
                    return Result(rid, "scenario", "BLOCKED", 0, f"fixture {c['fixture']} failed to build",
                                  c["spec"], c["xfail"])
            ctr = (self._with_gitpm(self._gitpmtest(self.runtime(), c["module"]))
                   .with_directory("/fx", fx)
                   .with_env_variable("GITPMT_RUN", run_id))
            return await self._run(sem, ctr, ["python3", "-m", "gitpmtest", "case", c["id"]], rid, "scenario",
                                   c["spec"], c["xfail"])

        results = list(await asyncio.gather(*(one(c) for c in cases)))
        return await self._fixture_results(get, registry, sorted(needed)) + results

    # -------------------------------------------------------------------- API

    @function
    async def fixtures(self) -> str:
        """Fixture graph: name, tier, number of cases, parents and module."""
        reg = await self._registry()
        users: dict[str, int] = {}
        for c in reg["cases"]:
            users[c["fixture"]] = users.get(c["fixture"], 0) + 1
        lines = [f"{'FIXTURE':24} {'TIER':6} {'CASES':>5}  REQUIRES (MODULE)"]
        for name, f in sorted(reg["fixtures"].items()):
            lines.append(f"{name:24} {'gitpm' if f['gitpm'] else 'source':6} {users.get(name, 0):>5}  "
                         f"{', '.join(f['requires']) or '-'} ({f['module']})")
        lines.append(f"{len(reg['fixtures'])} fixtures, {len(reg['cases'])} cases")
        return "\n".join(lines) + "\n"

    @function
    async def fixture(self, name: Annotated[str, Doc("fixture name")]) -> dagger.Directory:
        """One fixture snapshot (its /fx directory), for inspection with export."""
        return await self._fixture_builder(await self._registry())(name)

    @function(cache="never")
    async def test(
        self,
        only: Annotated[str, Doc("comma list of: rust, lint, scenarios (default: all)")] = "",
        filter: Annotated[str, Doc("substring of a test id, e.g. uc_015 or lifecycle; comma list = any")] = "",
        jobs: Annotated[int, Doc("tests running at once")] = 16,
        cached: Annotated[bool, Doc("reuse results of tests whose inputs did not change")] = False,
    ) -> dagger.Directory:
        """Run the tests concurrently; returns report.json, junit.xml, summary.txt, status and logs/."""
        kinds = {k.strip() for k in only.split(",") if k.strip()} or {"rust", "lint", "scenarios"}
        unknown = kinds - {"rust", "lint", "scenarios"}
        if unknown:
            raise ValueError(f"unknown --only value: {', '.join(sorted(unknown))}")
        run_id = "cached" if cached else uuid.uuid4().hex
        sem = asyncio.Semaphore(max(1, jobs))
        started = datetime.datetime.now(datetime.timezone.utc)
        groups = []
        if "rust" in kinds:
            groups.append(self._rust_tests(sem, run_id, filter))
        if "lint" in kinds:
            async def lint():
                if kinds & {"rust", "scenarios"}:
                    # One cold cargo build at a time: two in parallel ran the pb engine out of memory.
                    await self.build().sync()
                return await self._lint(sem, run_id, filter)
            groups.append(lint())
        if "scenarios" in kinds:
            async def scenarios():
                return await self._scenarios(sem, run_id, filter, await self._registry())
            groups.append(scenarios())
        results: list[Result] = []
        for chunk in await asyncio.gather(*groups, return_exceptions=True):
            if isinstance(chunk, BaseException):
                results.append(Result(f"setup:{type(chunk).__name__}", "setup", "FAIL", 0, _error_text(chunk)))
            else:
                results.extend(chunk)
        return _report(self.runtime(), results, started, self.commit)


def _match(filter: str, rid: str) -> bool:
    """filter: a substring of the test id, or several separated by commas (any matches)."""
    parts = [p for p in filter.split(",") if p]
    return not parts or any(p in rid for p in parts)


def _failed(get, name: str) -> bool:
    task = get(name)
    return task.done() and task.exception() is not None


def _error_text(e: BaseException) -> str:
    if isinstance(e, dagger.ExecError):
        return f"{e}\n--- stdout\n{e.stdout}\n--- stderr\n{e.stderr}"
    return f"{type(e).__name__}: {e}"


def _safe(rid: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "-_." else "_" for ch in rid)


UNBUNDLE = "import json, os\nfor path, text in json.load(open('/bundle.json')).items():\n    os.makedirs(os.path.dirname(f'/out/{path}') or '/out', exist_ok=True)\n    open(f'/out/{path}', 'w').write(text)\n"
STATUSES = ("PASS", "FAIL", "BLOCKED", "XFAIL", "XPASS")


def _report(runtime: dagger.Container, results: list[Result], started: datetime.datetime,
            commit: str) -> dagger.Directory:
    from xml.sax.saxutils import escape, quoteattr

    results.sort(key=lambda r: (r.status == "PASS", r.id))
    counts = {s: sum(r.status == s for r in results) for s in STATUSES}
    elapsed = (datetime.datetime.now(datetime.timezone.utc) - started).total_seconds()
    # One file per test written by a single exec: hundreds of with_new_file layers
    # exceed the overlayfs lower-dir limit when the directory is exported.
    files = {f"logs/{_safe(r.id)}.log": r.log for r in results}
    report = {"started": started.isoformat(), "seconds": round(elapsed, 1), "commit": commit, "counts": counts,
              "tests": [{"id": r.id, "kind": r.kind, "status": r.status, "ms": r.ms, "spec": r.spec,
                         "xfail": r.xfail or None, "log": f"logs/{_safe(r.id)}.log"} for r in results]}
    lines = []
    for r in results:
        if r.status not in ("PASS", "XFAIL"):
            tail = "\n".join("      " + l for l in r.log.strip().splitlines()[-15:])
            lines.append(f"{r.status:7} {r.id}  ({r.ms / 1000:.1f}s)\n{tail}")
    for r in results:
        if r.status == "XFAIL":
            lines.append(f"XFAIL   {r.id}  (until change {r.xfail})")
    by_kind: dict[str, list[int]] = {}
    for r in results:
        k = by_kind.setdefault(r.kind, [0, 0])
        k[0 if r.status == "PASS" else 1] += 1
    slowest = sorted(results, key=lambda r: -r.ms)[:5]
    lines.append("")
    lines.append(f"commit: {commit}")
    lines.append("slowest: " + ", ".join(f"{r.id} {r.ms / 1000:.1f}s" for r in slowest))
    lines.append("  ".join(f"{k}: {p}/{p + f}" for k, (p, f) in sorted(by_kind.items())))
    lines.append(f"TOTAL {len(results)}: {counts['PASS']} passed, {counts['FAIL']} failed, "
                 f"{counts['BLOCKED']} blocked, {counts['XFAIL']} xfail, {counts['XPASS']} xpass in {elapsed:.0f}s")
    # One suite per Rust test binary, per use case, and per remaining kind.
    suites: dict[str, list[tuple[str, Result]]] = {}
    for r in sorted(results, key=lambda r: r.id):
        if r.kind == "rust":
            suite, _, name = r.id[len("rust:"):].partition("::")
            suite = f"rust {suite}"
        elif r.kind == "scenario":
            module, _, name = r.id[len("scenario:"):].partition(":")
            suite = f"scenario {module}"
        else:
            suite, name = r.kind, r.id.split(":", 1)[-1]
        suites.setdefault(suite, []).append((name, r))
    failures = counts["FAIL"] + counts["XPASS"]
    junit = ['<?xml version="1.0" encoding="UTF-8"?>',
             f'<testsuites name="gitpm-itests" tests="{len(results)}" failures="{failures}" '
             f'errors="{counts["BLOCKED"]}" time="{elapsed:.1f}">']
    for suite, members in sorted(suites.items()):
        fails = sum(r.status in FAILING for _, r in members)
        errors = sum(r.status == "BLOCKED" for _, r in members)
        skipped = sum(r.status == "XFAIL" for _, r in members)
        seconds = sum(r.ms for _, r in members) / 1000
        junit.append(f'  <testsuite name={quoteattr(suite)} tests="{len(members)}" failures="{fails}" '
                     f'errors="{errors}" skipped="{skipped}" time="{seconds:.3f}">')
        for name, r in members:
            junit.append(f'    <testcase classname={quoteattr(suite)} name={quoteattr(name)} time="{r.ms / 1000:.3f}">')
            if r.status in FAILING:
                junit.append(f'      <failure message="{r.status.lower()}">{escape(r.log[-20000:])}</failure>')
            elif r.status == "BLOCKED":
                junit.append(f'      <error message="blocked">{escape(r.log[-20000:])}</error>')
            elif r.status == "XFAIL":
                junit.append(f'      <skipped message={quoteattr("xfail until " + r.xfail)}/>')
            else:
                junit.append(f'      <system-out>{escape(r.log[-4000:])}</system-out>')
            junit.append("    </testcase>")
        junit.append("  </testsuite>")
    junit.append("</testsuites>")
    files.update({"report.json": json.dumps(report, indent=1) + "\n", "summary.txt": "\n".join(lines) + "\n",
                  "junit.xml": "\n".join(junit) + "\n",
                  "status": "ok\n" if failures == 0 else "failed\n"})
    return (runtime.with_new_file("/bundle.json", json.dumps(files))
            .with_exec(["python3", "-c", UNBUNDLE])
            .directory("/out"))
