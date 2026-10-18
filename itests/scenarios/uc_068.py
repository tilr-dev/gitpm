"""UC-068: Consumer journey: install from catalog → lifecycle → uninstall (P0).

Fixture g6_journey: dan/hello 0.1.0 registered in /fx/catalog and installed from it.
Each step is its own case that first replays the earlier steps (`journey`), and
`whole_journey` checks that the author repository and the catalog are byte-identical
across all consumer steps.
"""

from gitpmtest import T, case

ID = "dan/hello"


def payload(t: T, version: str) -> str:
    return t.run("git", "-C", t.repo(ID), "show", f"{ID}@{version}:dist/index.js", check=True, quiet=True).out


def check_row(t: T) -> str:
    out = t.gitpm_ok("check").out
    for line in out.splitlines():
        if line.split()[:1] == [ID]:
            return line
    raise AssertionError(f"no check row for {ID}:\n{out}")


def at(t: T, version: str, previous: str | None, enabled: bool = True, catalog: bool = True):
    """State, clone HEAD and dist bytes match `version` (and `previous`); the catalog origin is kept."""
    p = t.package(ID)
    if catalog:
        t.eq(p.get("catalog"), "/fx/catalog/catalog.json", "catalog origin")
    commit = t.rev(ID, f"{ID}@{version}")
    t.eq(p["current"], {"version": version, "commit": commit}, "current")
    t.eq(p["previous"]["version"] if p["previous"] else None, previous, "previous")
    if previous:
        t.eq(p["previous"]["commit"], t.rev(ID, f"{ID}@{previous}"), "previous commit")
    t.eq(p["enabled"], enabled, "enabled")
    t.eq(t.rev(t.clone(ID)), commit, "clone HEAD")
    if enabled:
        t.eq((t.dist(ID) / "index.js").read_text(), payload(t, version), "dist bytes")
    else:
        t.ok(not t.dist(ID).exists(), "no dist while disabled")


def author_releases(t: T):
    """Author releases 0.1.1 and the catalog is updated; the consumer does not change."""
    before = t.snap(t.ws)
    t.release(ID, "0.1.1")
    t.gitpm_ok("register", t.repo(ID), "--ref", f"{ID}@0.1.1")
    t.same(before, "workspace (a catalog update alone)")


STEPS = ["release", "update", "rollback", "toggle", "forget"]


def journey(t: T, until: str):
    for step in STEPS[: STEPS.index(until)]:
        {"release": author_releases,
         "update": lambda t: t.gitpm_ok("update", ID),
         "rollback": lambda t: t.gitpm_ok("rollback", ID),
         "toggle": lambda t: (t.gitpm_ok("disable", ID), t.gitpm_ok("enable", ID)),
         "forget": lambda t: (t.gitpm_ok("forget", ID), t.gitpm_ok("restore", ID))}[step](t)


@case(fixture="g6_journey", spec=["install-uninstall/install-from-the-catalog", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#cli-table"])
def install_list_check(t: T):
    """install-catalog: state with catalog origin, clone HEAD, dist; list shows it; check says up to date."""
    at(t, "0.1.0", None)
    t.ok(t.package(ID)["catalog"].startswith(str(t.catalog)), "catalog recorded in state")
    listed = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    t.eq(listed[ID]["current"]["version"], "0.1.0", "list")
    t.has(check_row(t), "0.1.0", "up to date")


@case(fixture="g6_journey", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin"])
def catalog_update_alone_changes_nothing(t: T):
    """New release + catalog update: consumer unchanged; check shows update available 0.1.1 until update."""
    author_releases(t)
    before = t.snap(t.ws)
    row = check_row(t)
    t.has(row, "0.1.0", "0.1.1", "update available")
    t.has(check_row(t), "update available")
    t.same(before, "workspace (check)")
    at(t, "0.1.0", None)


@case(fixture="g6_journey", spec=["updates/stable-update-moves-to-the-newest-release"])
def update_to_new_release(t: T):
    """update: current 0.1.1, previous 0.1.0, catalog origin kept, check up to date."""
    catalog = t.package(ID)["catalog"]
    journey(t, "update")
    t.gitpm_ok("update", ID)
    at(t, "0.1.1", "0.1.0")
    t.eq(t.package(ID)["catalog"], catalog, "catalog origin unchanged")
    t.has(check_row(t), "0.1.1", "up to date")


@case(fixture="g6_journey", spec=["updates/rollback-swaps-current-and-previous#rollback-after-update"])
def rollback_swaps(t: T):
    """rollback swaps current and previous (payload back to 0.1.0)."""
    journey(t, "rollback")
    t.gitpm_ok("rollback", ID)
    at(t, "0.1.0", "0.1.1")
    t.has(check_row(t), "update available")


@case(fixture="g6_journey", spec=["dist/enable-and-disable-deploy-and-remove#disable-and-enable"])
def disable_enable_toggle_only_the_flag(t: T):
    """disable / enable change only `enabled` and remove / redeploy dist (same bytes)."""
    journey(t, "toggle")
    files = t.snap(t.clone(ID), t.dist(ID), git=True)
    t.gitpm_ok("disable", ID)
    at(t, "0.1.0", "0.1.1", enabled=False)
    t.gitpm_ok("enable", ID)
    at(t, "0.1.0", "0.1.1", enabled=True)
    t.same(files, "clone and dist")


@case(fixture="g6_journey", spec=["install-uninstall/three-removal-levels#forget-then-restore", "dist/forget-restore-and-uninstall-settle-dist"])
def forget_restore_without_history(t: T):
    """forget keeps the clone, removes dist and drops the entry; restore re-registers without
    previous and deploys the same dist bytes."""
    journey(t, "forget")
    files = t.snap(t.clone(ID), t.dist(ID), git=True)
    t.gitpm_ok("forget", ID)
    t.ok(ID not in t.state()["packages"], "entry forgotten")
    t.ok(not t.dist(ID).exists(), "dist removed while forgotten")
    t.gitpm_ok("restore", ID)
    # Restore re-registers retained files from the clone; the library records no catalog origin
    # for it (Installed { ..Default }), which the README does not promise either way.
    at(t, "0.1.0", None, catalog=False)
    print(f"  note: catalog after restore = {t.package(ID).get('catalog')!r}")
    print(f"  note: check after restore: {check_row(t)}")
    t.same(files, "clone and dist after restore")


@case(fixture="g6_journey", spec=["install-uninstall/three-removal-levels", "install-uninstall/only-manager-owned-files-are-deleted"])
def whole_journey(t: T):
    """The full journey ends with uninstall removing entry, clone and dist; the author repository and the
    catalog are byte-identical across all consumer steps."""
    author_releases(t)
    author = t.snap(t.repo(ID), git=True)
    catalog = t.snap(t.catalog)
    t.has(check_row(t), "update available")
    t.gitpm_ok("update", ID)
    at(t, "0.1.1", "0.1.0")
    t.gitpm_ok("rollback", ID)
    at(t, "0.1.0", "0.1.1")
    t.gitpm_ok("disable", ID)
    t.gitpm_ok("enable", ID)
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    at(t, "0.1.0", None, catalog=False)
    t.gitpm_ok("uninstall", ID)
    t.not_installed(ID)
    t.same(author, "author repository")
    t.same(catalog, "catalog")
