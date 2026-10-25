"""UC-032: Catalog generation validation and retry (P0).

Fixture g4_sources_catalog: /fx/catalog generated from /fx/lists/sources.txt (3dce1568-569c-5b53-9321-37185e751947
0.1.0-0.2.0, 2bf18134-a7da-5c98-841c-4ee91fa6e90e 1.0.0-1.1.0). Each case adds a valid new release 3dce1568-569c-5b53-9321-37185e751947 0.3.0
together with one bad input, checks that nothing at all is written, fixes the input and
retries.
"""

import json

from gitpmtest import T, case

HELLO = "3dce1568-569c-5b53-9321-37185e751947"
SOURCES = "lists/sources.txt"


def update(t: T, ok=True, **kw):
    args = ("catalog-update", "--sources", t.root / SOURCES)
    return t.gitpm_ok(*args) if ok else t.gitpm_err(*args, **kw)


def add_source(t: T, line: str):
    with open(t.root / SOURCES, "a") as f:
        f.write(line + "\n")


def remove_line(t: T, line: str):
    path = t.root / SOURCES
    path.write_text("".join(l for l in path.read_text().splitlines(True) if l.strip() != line))


def failed_then_retry(t: T, bad, fix, kind: str, contains):
    commit = t.release(HELLO, "0.3.0")
    bad()
    before = t.snap(t.catalog)
    update(t, ok=False, kind=kind, contains=contains)
    t.same(before, "catalog after failed catalog-update")
    t.ok(not t.catalog_release(HELLO, "0.3.0").exists(), "valid new release not partially published")
    fix()
    update(t)
    t.eq(json.loads(t.catalog_release(HELLO, "0.3.0").read_text())["commit"], commit, "retry publishes 0.3.0")
    t.eq(sorted(json.loads(t.catalog_index(HELLO).read_text())["versions"]),
         sorted(["0.1.0", "0.1.1", "0.2.0", "0.3.0"]), "hello versions after retry")


@case(fixture="g4_sources_catalog", spec=["catalog/catalog-generated-from-git-release-tags", "catalog/writers-are-serialized-and-crash-safe"])
def missing_second_source(t: T):
    """A second source path that does not exist: refused before any write; retry after
    removing it succeeds."""
    failed_then_retry(t, lambda: add_source(t, "../repos/no/such-repo"),
                      lambda: remove_line(t, "../repos/no/such-repo"),
                      "invalid_argument", "local source does not exist")


@case(fixture="g4_sources_catalog", spec=["catalog/catalog-generated-from-git-release-tags", "catalog/writers-are-serialized-and-crash-safe"])
def unreachable_second_source(t: T):
    """A network source that cannot be reached (closed loopback port): refused, nothing
    written; retry without it succeeds."""
    url = "http://127.0.0.1:9/bad.git"
    failed_then_retry(t, lambda: add_source(t, url), lambda: remove_line(t, url), "git", ())


@case(fixture="g4_sources_catalog", spec=["package-ids/broken-historical-tags-are-skipped", "package-ids/release-tag-names#tag-disagrees-with-the-manifest"])
def broken_latest_tag_in_second_source(t: T):
    """86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6: valid 1.0.0 plus tag v2.0.0 whose manifest says 1.0.1 (broken latest):
    refused (version_conflict), neither 86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6 1.0.0 nor hello 0.3.0 written; retry after
    deleting the broken tag publishes both."""
    def bad():
        t.new_package("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "1.0.0")
        t.release("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "1.0.1", tag=False)
        t.tag("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "v2.0.0")
        add_source(t, "../repos/86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6")

    def fix():
        t.git("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "tag", "-d", "v2.0.0")

    failed_then_retry(t, bad, fix, "version_conflict", "latest release tag is broken")
    t.ok(t.catalog_release("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "1.0.0").exists(), "86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6 1.0.0 published on retry")


@case(fixture="g4_sources_catalog", spec=["package-ids/broken-historical-tags-are-skipped"])
def malformed_manifest_tag_newer_than_valid(t: T):
    """A newer release tag with unparsable plugin.json in the second source: refused;
    retry after moving that tag away succeeds."""
    def bad():
        t.new_package("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "1.0.0")
        t.commit("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "broken", {"plugin.json": "{ not json"})
        t.tag("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "v1.1.0")
        t.commit("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "repair", {"plugin.json": json.dumps({"id": "86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "name": "fixture", "version": "1.0.0"})})
        add_source(t, "../repos/86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6")

    failed_then_retry(t, bad, lambda: t.git("86ca4dbd-0966-543b-a2cf-a2d5ea1d22f6", "tag", "-d", "v1.1.0"),
                      "version_conflict", "latest release tag is broken")


@case(fixture="g4_sources_catalog", spec=["catalog/published-versions-are-immutable#regenerate", "package-ids/tags-are-not-policed-integrity-via-protected-catalog#moved-tag-on-regenerate"])
def moved_old_release(t: T):
    """An already published release tag (2bf18134-a7da-5c98-841c-4ee91fa6e90e@1.0.0) moved to another commit: the
    published record is kept with a warning, and hello 0.3.0 is published (change
    no-tag-immutability)."""
    spaced = t.root / "spaced repos/2bf18134-a7da-5c98-841c-4ee91fa6e90e"
    original = t.rev(spaced, "v1.0.0")
    published = t.catalog_release("2bf18134-a7da-5c98-841c-4ee91fa6e90e", "1.0.0").read_bytes()
    commit = t.release(HELLO, "0.3.0")
    t.git(spaced, "checkout", "-q", "-b", "side", original)
    moved = t.commit(spaced, "rewritten 1.0.0", {"dist/index.js": "rewritten\n"})
    t.tag(spaced, "v1.0.0", force=True)
    t.git(spaced, "checkout", "-q", "main")
    r = update(t)
    t.has(r.err, f"Warning: release 2bf18134-a7da-5c98-841c-4ee91fa6e90e@1.0.0: tag now points to {moved[:12]}; the published record is kept")
    t.eq(t.catalog_release("2bf18134-a7da-5c98-841c-4ee91fa6e90e", "1.0.0").read_bytes(), published, "published 1.0.0 bytes")
    t.eq(json.loads(t.catalog_release(HELLO, "0.3.0").read_text())["commit"], commit, "0.3.0 published")


@case(fixture="g4_sources_catalog", spec=["catalog/published-versions-are-immutable#manifest-or-files-differ-from-the-published-record"])
def edited_published_record_is_kept(t: T):
    """A published record whose file hashes differ from the regenerated release (a manual
    edit of release.json) is kept with a warning; catalog-update exits 0."""
    path = t.catalog_release("2bf18134-a7da-5c98-841c-4ee91fa6e90e", "1.0.0")
    record = json.loads(path.read_text())
    name = next(iter(record["files"]))
    record["files"][name]["sha256"] = "ab" * 32
    path.write_text(json.dumps(record, indent=2) + "\n")
    edited = path.read_bytes()
    r = update(t)
    t.has(r.err, "Warning: release 2bf18134-a7da-5c98-841c-4ee91fa6e90e@1.0.0 differs from the published record; the published record is kept")
    t.eq(path.read_bytes(), edited, "edited record kept")


@case(fixture="g4_sources_catalog", spec=["catalog/published-versions-are-immutable#regenerate"])
def removed_source_entries_remain(t: T):
    """Removing 2bf18134-a7da-5c98-841c-4ee91fa6e90e from sources.txt and regenerating keeps all its catalog files."""
    spaced = t.snap(t.catalog / "packages/2bf18134-a7da-5c98-841c-4ee91fa6e90e")
    remove_line(t, "../spaced repos/2bf18134-a7da-5c98-841c-4ee91fa6e90e")
    t.release(HELLO, "0.3.0")
    out = update(t)
    t.has(out.out, "Catalog ready: 4 release(s)")
    t.same(spaced, "removed source's entries")
    t.ok("spaced" in json.loads((t.catalog / "packages/dan/catalog.json").read_text())["directories"],
         "directory link kept")
    t.ok(t.catalog_release(HELLO, "0.3.0").exists(), "remaining source updated")


@case(fixture="g4_sources_catalog", spec=["catalog/published-versions-are-immutable#regenerate"])
def build_metadata_preserved(t: T):
    """A release registered with build metadata keeps it (byte-identical) when
    catalog-update rediscovers the same tag."""
    t.release(HELLO, "0.3.0")
    build = {"url": "https://pkg.example.invalid/hello-0.3.0.tar.zst", "sha256": "ab" * 32}
    t.gitpm_ok("register", t.repo(HELLO), "--ref", f"v0.3.0",
            "--build-url", build["url"], "--build-sha256", build["sha256"])
    before = t.snap(t.catalog)
    update(t)
    t.same(before, "catalog")
    t.eq(json.loads(t.catalog_release(HELLO, "0.3.0").read_text())["build"], build, "build kept")
