"""UC-011: Stable release selection (P0).

Fixture g2_reverse_tags: 1.10.0 is committed and tagged before 1.9.0, one of them with an
annotated and the other with a lightweight tag, and an untagged 1.11.0 HEAD on top.
"""

from gitpmtest import T, case


def check_numeric_max(t: T, id: str):
    tagged = t.rev(id, f"v1.10.0")
    p = t.package(id)
    t.eq(p["current"], {"version": "1.10.0", "commit": tagged}, "current (numeric maximum)")
    t.ok(p["current"]["commit"] != t.rev(id), "untagged HEAD (1.11.0) not chosen")
    t.ok(p["current"]["commit"] != t.rev(id, f"v1.9.0"), "newest tagged commit (1.9.0) not chosen")
    t.eq(t.rev(t.clone(id)), tagged, "clone HEAD")
    t.eq((t.dist(id) / "index.js").read_text(), f'export const greeting = "{id} 1.10.0";\n', "dist")


@case(fixture="g2_reverse_tags", spec=["install-uninstall/install-the-latest-release-of-a-git-source#newest-numeric-tag-wins", "package-ids/version-format#numeric-ordering"])
def install_without_ref_picks_numeric_max(t: T):
    """No --ref: annotated 1.10.0 beats the later lightweight 1.9.0 and the untagged HEAD."""
    t.gitpm_ok("install", t.repo("d0a24c12-6c98-54aa-9226-f2a923716f63"))
    check_numeric_max(t, "d0a24c12-6c98-54aa-9226-f2a923716f63")


@case(fixture="g2_reverse_tags", spec=["install-uninstall/install-the-latest-release-of-a-git-source#newest-numeric-tag-wins"])
def install_picks_numeric_max_with_lightweight_tags(t: T):
    """No --ref: lightweight 1.10.0 beats the later annotated 1.9.0 and the untagged HEAD."""
    t.gitpm_ok("install", t.repo("ac004289-237b-5412-9383-e963b2c10d1d"))
    check_numeric_max(t, "ac004289-237b-5412-9383-e963b2c10d1d")


@case(fixture="g2_reverse_tags", spec=["install-uninstall/install-the-latest-release-of-a-git-source#newest-numeric-tag-wins"])
def both_forms_in_one_workspace(t: T):
    """Install and update --release select the same release for each tag-form mix."""
    t.gitpm_ok("install", t.repo("d0a24c12-6c98-54aa-9226-f2a923716f63"))
    t.gitpm_ok("install", t.repo("ac004289-237b-5412-9383-e963b2c10d1d"))
    for id in ["d0a24c12-6c98-54aa-9226-f2a923716f63", "ac004289-237b-5412-9383-e963b2c10d1d"]:
        r = t.gitpm_ok("update", id, "--release")
        t.has(r.out, f"{id} up to date (1.10.0)")
        t.ok("mode" not in t.package(id), f"{id}: release mode records no mode")
    check_numeric_max(t, "d0a24c12-6c98-54aa-9226-f2a923716f63")
    check_numeric_max(t, "ac004289-237b-5412-9383-e963b2c10d1d")


@case(fixture="g2_reverse_tags", spec=["install-uninstall/install-the-latest-release-of-a-git-source#repository-without-releases"])
def repository_without_release_tags_fails(t: T):
    """No release tags (only unrelated tags): stable install fails instead of installing HEAD."""
    id = "9ca55a49-b6df-5726-8a0c-b1bafd1d4ef6"
    t.new_package(id, "0.1.0", tag=False)
    t.release(id, "0.2.0", tag=False)
    t.tag(id, "wip")
    t.tag(id, "v1.0.0")
    t.gitpm_err("install", t.repo(id), kind="no_releases", contains=("no release tags", id))
    t.not_installed(id)
