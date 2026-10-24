"""UC-030: Build metadata validation (P1).

Build metadata is immutable. Installations prefer the verified archive and refuse
unavailable or corrupt artifacts without changing the workspace.
"""

import json
import os

from gitpmtest import T, case

ID = "dan/hello"
SHA = "ab" * 32
URL = "https://pkg.example.invalid/builds/hello-0.3.0.tar.zst"


def new_release(t: T) -> str:
    return t.release(ID, "0.3.0")


def register(t: T, *extra, ref=f"v0.3.0"):
    return ("register", t.repo(ID), "--ref", ref, *extra)


def stored(t: T, version="0.3.0") -> dict:
    return json.loads(t.catalog_release(ID, version).read_text())


def refused(t: T, *extra, kind=None, contains=()):
    """A rejected registration leaves the catalog byte-identical and installs nothing."""
    before = t.snap(t.catalog)
    r = t.gitpm_err(*register(t, *extra), kind=kind, contains=contains)
    t.same(before, "catalog")
    t.ok(not t.catalog_release(ID, "0.3.0").exists(), "0.3.0 not published")
    t.ok(not t.ws.exists(), "nothing installed")
    return r


@case(fixture="catalog", spec=["catalog/register-one-version"])
def valid_pair_is_stored(t: T):
    """Paired HTTPS --build-url and 64-hex --build-sha256 are stored verbatim in release.json."""
    new_release(t)
    out = t.gitpm_ok(*register(t, "--build-url", URL, "--build-sha256", SHA)).json()
    t.eq(stored(t)["build"], {"url": URL, "sha256": SHA}, "stored build")
    t.eq(out["build"], {"url": URL, "sha256": SHA}, "printed build")
    t.ok(stored(t)["files"], "file hashes still computed from Git")


@case(fixture="catalog", spec=["catalog/register-one-version#invalid-build-metadata"])
def url_alone_is_rejected(t: T):
    new_release(t)
    refused(t, "--build-url", URL, contains="--build-url and --build-sha256 must be provided together")


@case(fixture="catalog", spec=["catalog/register-one-version#invalid-build-metadata"])
def sha_alone_is_rejected(t: T):
    new_release(t)
    refused(t, "--build-sha256", SHA, contains="--build-url and --build-sha256 must be provided together")


@case(fixture="catalog", spec=["catalog/register-one-version#invalid-build-metadata"])
def malformed_hash_is_rejected(t: T):
    """63 hex chars, a non-hex char and an empty value are all refused."""
    new_release(t)
    for bad in [SHA[:-1], SHA[:-1] + "g", ""]:
        refused(t, "--build-url", URL, "--build-sha256", bad, kind="catalog", contains="invalid build SHA-256")


@case(fixture="catalog", spec=["catalog/register-one-version#invalid-build-metadata"])
def non_https_url_is_rejected(t: T):
    new_release(t)
    for bad in ["http://pkg.example.invalid/x.tar.zst", "ftp://pkg.example.invalid/x", "https://", "file:///tmp/x"]:
        refused(t, "--build-url", bad, "--build-sha256", SHA, kind="catalog", contains="HTTPS")


@case(fixture="catalog", spec=["cli/option-validation"])
def build_on_other_command_is_rejected(t: T):
    """Build options belong to register only (usage error, nothing written)."""
    before = t.snap(t.catalog)
    t.gitpm_err("install-catalog", ID, "--build-url", URL, "--build-sha256", SHA, contains="build options require catalog register")
    t.same(before, "catalog")
    t.ok(not t.ws.exists(), "nothing installed")


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable#register-with-another-build"])
def changing_existing_build_is_rejected(t: T):
    """A registered build cannot be changed (url or sha) nor dropped; identical repeat is fine."""
    new_release(t)
    t.gitpm_ok(*register(t, "--build-url", URL, "--build-sha256", SHA))
    before = t.snap(t.catalog)
    t.gitpm_ok(*register(t, "--build-url", URL, "--build-sha256", SHA))
    t.same(before, "catalog after identical repeat")
    for extra in [("--build-url", URL + ".new", "--build-sha256", SHA),
                  ("--build-url", URL, "--build-sha256", "cd" * 32),
                  ()]:
        t.gitpm_err(*register(t, *extra), kind="version_conflict", contains="immutable")
        t.same(before, f"catalog after refused change {extra}")
    t.eq(stored(t)["build"], {"url": URL, "sha256": SHA}, "build unchanged")


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable#register-with-another-build"])
def adding_build_to_existing_release_is_rejected(t: T):
    """dan/hello 0.2.0 was registered without build; adding build metadata later is refused."""
    before = t.snap(t.catalog)
    t.gitpm_err(*register(t, "--build-url", URL, "--build-sha256", SHA, ref=f"v0.2.0"),
             kind="version_conflict", contains="immutable")
    t.same(before, "catalog")
    t.ok("build" not in stored(t, "0.2.0"), "0.2.0 still without build")


@case(fixture="catalog", spec=["catalog/nothing-is-built-executed-or-uploaded", "install-uninstall/install-from-the-catalog"])
def install_refuses_unreachable_build(t: T):
    """A build URL is authoritative; a failed request cannot fall back to Git."""
    new_release(t)
    t.gitpm_ok(*register(t, "--build-url", "https://127.0.0.1:9/never/hello.tar.gz", "--build-sha256", "0" * 64))
    log = t.root / "curl.log"
    stub = t.root / "stub-bin/curl"
    stub.parent.mkdir()
    stub.write_text(f'#!/bin/sh\necho "$@" >> {log}\nexit 7\n')
    stub.chmod(0o755)
    path = f"{stub.parent}:{os.environ['PATH']}"
    before = t.snap(t.ws) if t.ws.exists() else None
    t.gitpm_err("install-catalog", ID, "--version", "0.3.0", env={"PATH": path}, kind="http")
    t.ok(log.exists(), "artifact request made")
    t.ok(not t.clone(ID).exists(), "no clone created")
    t.ok(not t.dist(ID).exists(), "nothing deployed")
    if before is not None:
        t.same(before, "workspace")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "updates/rollback-swaps-current-and-previous"])
def builds_install_update_and_rollback_without_clones(t: T):
    """Real loopback artifacts; unavailable Git source, ordinary and verified catalogs."""
    import hashlib
    import http.server
    import io
    import tarfile
    import threading

    served = t.root / "artifacts"
    served.mkdir()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0),
        lambda *a, **kw: http.server.SimpleHTTPRequestHandler(*a, directory=str(served), **kw))
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    url = f"http://127.0.0.1:{server.server_address[1]}"
    try:
        for version in ["0.3.0", "0.4.0"]:
            t.release(ID, version)
            contents = (t.git(ID, "show", f"v{version}:dist/index.js") + "\n").encode()
            artifact = served / f"{version}.tar.gz"
            with tarfile.open(artifact, "w:gz") as archive:
                entry = tarfile.TarInfo("index.js")
                entry.size = len(contents)
                entry.mode = 0o755
                archive.addfile(entry, io.BytesIO(contents))
            t.gitpm_ok(*register(t, "--build-url", f"{url.replace('http://', 'https://')}/{artifact.name}",
                "--build-sha256", hashlib.sha256(artifact.read_bytes()).hexdigest(), ref=f"v{version}"))
            record = stored(t, version)
            record["build"]["url"] = f"{url}/{artifact.name}"
            t.catalog_release(ID, version).write_text(json.dumps(record))
        t.repo(ID).rename(t.root / "source-unavailable")
        t.gitpm_ok("install-catalog", ID, "--version", "0.3.0")
        t.ok(not t.clone(ID).exists(), "no Git clone")
        t.eq(t.package(ID)["current"]["version"], "0.3.0", "explicit build version")
        t.gitpm_ok("update", ID)
        t.eq(t.package(ID)["current"]["version"], "0.4.0", "updated from build")
        t.ok(not t.clone(ID).exists(), "update creates no clone")
        for artifact in served.iterdir():
            artifact.unlink()
        t.gitpm_ok("rollback", ID)
        t.eq(t.package(ID)["current"]["version"], "0.3.0", "rollback needs no artifact request")
        t.gitpm_ok("disable", ID)
        t.gitpm_ok("enable", ID)
        t.ok(t.dist(ID).is_dir(), "cached build redeployed")
        t.ok(not t.clone(ID).exists(), "lifecycle creates no clone")
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
