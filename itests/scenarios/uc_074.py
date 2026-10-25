"""UC-074: install is idempotent for the same source (gitpm 0.5).

Installing an installed package again from the same source prints `ID already installed
(VERSION)` and exits 0 without touching the workspace; `; run update` (or the requested
ref or --dev) is appended when the source resolves to another revision. Another source for
the same ID is still refused with `already_installed`.

Fixtures: hello_installed (dan/hello 0.1.0 from /fx/repos/dan/hello), g1_ws_catalog
(dan/hello 0.1.1 installed from the catalog, 0.2.0 available there).
"""

from gitpmtest import T, case

ID = "dan/hello"


def noop(t: T, *args, out: str):
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok(*args)
    t.eq(r.out, out, "output")
    t.eq(r.err, "", "no stderr")
    t.same(before, "workspace (byte-identical)")


@case(fixture="hello_installed", spec=["install-uninstall/install-is-idempotent-for-the-same-source#repeat-install"])
def repeat_install_is_a_noop(t: T):
    """Same source, no newer release: `dan/hello already installed (0.1.0)`, exit 0, nothing changes."""
    noop(t, "install", t.repo(ID), out=f"{ID} already installed (0.1.0)\n")
    # The same revision by tag asks for another mode (pinned): the hint names it.
    noop(t, "install", t.repo(ID), "--ref", f"v0.1.0",
         out=f"{ID} already installed (0.1.0); requested --ref v0.1.0 — run update {ID} --ref v0.1.0\n")


@case(fixture="hello_installed", spec=["install-uninstall/install-is-idempotent-for-the-same-source#newer-release-available"])
def newer_release_hints_update(t: T):
    """A newer release upstream: the line ends with `; run update`; install does not move to it."""
    t.release(ID, "0.1.1")
    noop(t, "install", t.repo(ID), out=f"{ID} already installed (0.1.0); run update\n")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "still 0.1.0")


@case(fixture="hello_installed", spec=["install-uninstall/install-is-idempotent-for-the-same-source"])
def requested_ref_or_dev_is_named(t: T):
    """--ref / --dev naming another revision: the hint repeats them for update."""
    t.release(ID, "0.1.1")
    noop(t, "install", t.repo(ID), "--ref", f"v0.1.1",
         out=f"{ID} already installed (0.1.0); requested --ref v0.1.1 — run update {ID} --ref v0.1.1\n")
    t.commit(ID, "unreleased", {"dist/index.js": "dev\n"})
    noop(t, "install", t.repo(ID), "--dev",
         out=f"{ID} already installed (0.1.0); requested --dev — run update {ID} --dev\n")


@case(fixture="hello_installed", spec=["install-uninstall/same-id-from-another-source-is-refused#other-source"])
def other_source_is_refused(t: T):
    """A clone of the repository elsewhere is another source: already_installed, nothing changes."""
    mirror = t.root / "mirror/hello"
    t.run("git", "clone", "-q", t.repo(ID), mirror, check=True)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", mirror, kind="already_installed", contains=ID)
    t.gitpm_err("install", f"file://{t.repo(ID)}", kind="already_installed", contains=ID)
    t.same(before, "workspace")


@case(fixture="g1_ws_catalog", spec=["install-uninstall/install-is-idempotent-for-the-same-source",
                                     "install-uninstall/install-from-the-catalog"])
def install_catalog_again_is_a_noop(t: T):
    """install-catalog of the installed version: already installed, exit 0, workspace and catalog unchanged."""
    catalog = t.snap(t.catalog)
    noop(t, "install-catalog", ID, "--version", "0.1.1", out=f"{ID} already installed (0.1.1)\n")
    t.same(catalog, "catalog")
