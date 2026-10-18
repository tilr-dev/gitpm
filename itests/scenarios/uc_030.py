"""UC-030: Build metadata validation (P1).

Archive transport is NOT implemented: build metadata is only stored; install-catalog
keeps using Git. The no-download check puts a logging `curl` first on PATH (gitpm reads
HTTP catalogs through curl), so any archive request would be recorded.
"""

import json
import os

from gitpmtest import T, case

ID = "dan/hello"
SHA = "ab" * 32
URL = "https://pkg.example.invalid/builds/hello-0.3.0.tar.zst"


def new_release(t: T) -> str:
    return t.release(ID, "0.3.0")


def register(t: T, *extra, ref=f"{ID}@0.3.0"):
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
    t.gitpm_err("install-catalog", ID, "--build-url", URL, "--build-sha256", SHA, contains="build options require register")
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
    t.gitpm_err(*register(t, "--build-url", URL, "--build-sha256", SHA, ref=f"{ID}@0.2.0"),
             kind="version_conflict", contains="immutable")
    t.same(before, "catalog")
    t.ok("build" not in stored(t, "0.2.0"), "0.2.0 still without build")


@case(fixture="catalog", spec=["catalog/nothing-is-built-executed-or-uploaded", "install-uninstall/install-from-the-catalog"])
def install_ignores_unreachable_build(t: T):
    """A release whose build URL is unreachable and whose archive hash is arbitrary installs
    from Git; no HTTP request is made and archive bytes are never checked."""
    commit = new_release(t)
    t.gitpm_ok(*register(t, "--build-url", "https://127.0.0.1:9/never/hello.tar.zst", "--build-sha256", "0" * 64))
    log = t.root / "curl.log"
    stub = t.root / "stub-bin/curl"
    stub.parent.mkdir()
    stub.write_text(f"#!/bin/sh\necho \"$@\" >> {log}\nexit 7\n")
    stub.chmod(0o755)
    path = f"{stub.parent}:{os.environ['PATH']}"
    t.gitpm_ok("install-catalog", ID, "--version", "0.3.0", env={"PATH": path})
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.3.0", "commit": commit}, "installed from Git at the catalog commit")
    t.eq(t.rev(t.clone(ID)), commit, "clone HEAD")
    t.eq((t.dist(ID) / "index.js").read_text(), t.git(ID, "show", f"{ID}@0.3.0:dist/index.js") + "\n", "dist bytes")
    t.ok(not log.exists(), "no HTTP request (curl never called)")
    t.eq(sorted(p.name for p in t.dist(ID).iterdir()), ["index.js"], "no archive in dist")
