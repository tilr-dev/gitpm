"""Controlled Cargo freshness regression, run on the same engine as the suite."""

import asyncio
import dataclasses
import json
import uuid

from dagger import dag


async def check(runner):
    # Equal-sized sources and old, identical mtimes deliberately defeat Cargo's
    # mtime-based freshness check when two snapshots share a target directory.
    nonce = uuid.uuid4().hex
    common = (dag.directory()
              .with_new_file("Cargo.toml", '[workspace]\nmembers = ["cli", "api"]\nresolver = "2"\n')
              .with_new_file("Cargo.lock", 'version = 4\n[[package]]\nname = "gitpm-cli"\nversion = "0.1.0"\n[[package]]\nname = "gitpm"\nversion = "0.1.0"\n')
              .with_new_file("cli/Cargo.toml", '[package]\nname = "gitpm-cli"\nversion = "0.1.0"\nedition = "2021"\n[[bin]]\nname = "gitpm"\npath = "src/main.rs"\n')
              .with_new_file("api/Cargo.toml", '[package]\nname = "gitpm"\nversion = "0.1.0"\nedition = "2021"\n')
              .with_new_file("api/src/lib.rs", "// " + nonce + "\n"))

    def source(marker):
        code = f'fn main() {{ println!("{marker}"); }}\n#[test] fn source_{marker}() {{}}\n'
        return (common.with_new_file("cli/src/main.rs", code)
                .with_new_file("api/examples/api-parity.rs", code))

    async def build(marker, phase):
        from .main import BUILD

        # Spec edits must not invalidate the build source namespace.
        tree = source(marker).with_new_file("openspec/specs/probe/spec.md", phase)
        own = dataclasses.replace(runner, source=tree)
        ctr = (await own._rust("gitpm-itests-cache-regression-" + nonce))
        ctr = (ctr.with_env_variable("REGRESSION_PHASE", phase)
               .with_exec(["sh", "-ec", "find /src/gitpm -type f -exec touch -d @1 {} +"])
               .with_new_file("/usr/local/bin/gitpmt-build", BUILD, permissions=0o755)
               .with_exec(["gitpmt-build"]))
        out = ctr.directory("/out")
        runtime = runner.runtime().with_directory("/out", out)
        for binary in ("gitpm", "gitpm-api"):
            got = (await runtime.with_exec(["/out/bin/" + binary]).stdout()).strip()
            assert got == marker, f"{phase}: {binary}: expected {marker}, got {got}"
        suites = json.loads(await out.file("manifest.json").contents())
        cli = next(s for s in suites if s["suite"] == "cli/bin")
        assert cli["tests"] == ["source_" + marker], f"{phase}: stale test executable: {cli}"
        await runtime.with_exec([cli["exe"]]).sync()
        return ctr

    await build("a", "seed")
    await build("b", "replace")
    await asyncio.gather(build("a", "parallel"), build("b", "parallel"))
    # Force a new exec (not a Dagger layer hit), leaving Cargo's outputs intact.
    warm = await build("b", "reuse")
    log = await warm.stderr()
    assert "Compiling " not in log, f"unchanged sources did not reuse compilation: {log}"
    return "source-owned CLI, API and Rust tests: sequential, parallel, warm cache OK"
