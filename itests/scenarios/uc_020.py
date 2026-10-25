"""UC-020: Enable and disable (P1).

Fixture g2_demos_installed: two unrelated packages without dependencies,
A = dd3387ed-a5a3-5059-81ad-b1f6987a28f2 and B = be517c52-cf84-5615-9a18-9fae7d82c819.
"""

import json
import shutil

from gitpmtest import T, case

A, B = "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"


def state_text(t: T) -> str:
    return (t.ws / "state.json").read_text()


def files(t: T):
    """Every clone (full .git) and dist of A and B."""
    return t.snap(t.clone(A), t.dist(A), t.clone(B), t.dist(B), git=True)


def expect_only_enabled_changed(t: T, before: str, enabled: bool):
    """state.json equals `before` with packages.A.enabled set, byte for byte (same serializer layout)."""
    value = json.loads(before)
    t.eq(json.dumps(value, indent=2), before, "state.json layout is reproducible (precondition)")
    value["packages"][A]["enabled"] = enabled
    t.eq(state_text(t), json.dumps(value, indent=2), f"state.json with only packages.{A}.enabled = {enabled}")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove", "dist/empty-parent-folders-are-removed"])
def disable_changes_only_enabled_flag(t: T):
    """disable A: only packages.A.enabled flips in state.json, dist/A is removed; clones and B's dist identical."""
    before = state_text(t)
    payload = t.snap(t.clone(A), t.clone(B), t.dist(B), git=True)
    t.gitpm_ok("disable", A)
    expect_only_enabled_changed(t, before, False)
    t.same(payload, "clones of A and B, dist of B")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.ok(not (t.ws / "dist/vendor").exists(), "empty namespace folder dist/vendor removed")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove", "dist/repeating-enable-and-disable-is-idempotent#repeat-disable"])
def repeated_disable_is_noop(t: T):
    """disable A twice: the second call exits 0 and changes nothing."""
    t.gitpm_ok("disable", A)
    before = t.snap(t.ws, git=True)
    t.gitpm_ok("disable", A)
    t.same(before, "workspace after the second disable")
    t.eq(t.package(A)["enabled"], False, "A disabled")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove#disable-and-enable", "dist/repeating-enable-and-disable-is-idempotent#repeat-enable"])
def enable_restores_and_repeated_enable_is_noop(t: T):
    """disable then enable A restores state.json byte for byte; a second enable changes nothing."""
    original, payload = state_text(t), files(t)
    t.gitpm_ok("disable", A)
    disabled = state_text(t)
    t.gitpm_ok("enable", A)
    expect_only_enabled_changed(t, disabled, True)
    t.eq(state_text(t), original, "state.json back to the original bytes")
    before = t.snap(t.ws, git=True)
    t.gitpm_ok("enable", A)
    t.same(before, "workspace after the second enable")
    t.same(payload, "clones and dist of A and B")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove", "dist/repeating-enable-and-disable-is-idempotent#repeat-enable"])
def enable_of_enabled_plugin_is_noop(t: T):
    """enable an already enabled A: exit 0, nothing changes."""
    before = t.snap(t.ws, git=True)
    t.gitpm_ok("enable", A)
    t.same(before, "workspace")


@case(fixture="g2_demos_installed", spec=["dist/repeating-enable-and-disable-is-idempotent#repeat-enable-repairs-missing-dist"])
def repeated_enable_repairs_missing_dist(t: T):
    """enable an already enabled A while dist/B is missing: dist/B comes back, state.json and clones unchanged."""
    state, payload = state_text(t), files(t)
    clones = t.snap(t.clone(A), t.clone(B), git=True)
    shutil.rmtree(t.dist(B))
    t.gitpm_ok("enable", A)
    t.eq(state_text(t), state, "state.json unchanged")
    t.same(clones, "clones of A and B")
    t.same(payload, "clones and dist of A and B (dist/B restored)")


@case(fixture="g2_demos_installed", spec=["errors/stable-error-kinds#kind-of-a-missing-package"])
def unknown_id_is_not_installed(t: T):
    """disable/enable of an unknown ID: not_installed naming the ID, nothing changes."""
    before = t.snap(t.ws, git=True)
    for command in ["disable", "enable"]:
        t.gitpm_err(command, "a68376c0-a262-545a-a4f8-afe9998880a6", kind="not_installed", contains="package a68376c0-a262-545a-a4f8-afe9998880a6 is not installed")
        t.same(before, f"workspace after {command} of an unknown ID")


@case(fixture="g2_demos_installed", spec=["dist/inactive-packages-change-state-and-clone-only"])
def update_of_disabled_plugin_keeps_it_disabled(t: T):
    """Disable A, release A 0.2.0, update A: succeeds, enabled stays false, no dist until enable
    (which deploys 0.2.0); B untouched."""
    t.gitpm_ok("disable", A)
    b = t.snap(t.clone(B), t.dist(B), git=True)
    b_entry = t.package(B)
    commit = t.release(A, "0.2.0")
    t.gitpm_ok("update", A)
    p = t.package(A)
    t.eq(p["enabled"], False, "still disabled")
    t.eq(p["current"], {"version": "0.2.0", "commit": commit}, "current")
    t.eq(t.rev(t.clone(A)), commit, "clone HEAD")
    t.ok(not t.dist(A).exists(), "no dist/A while disabled")
    t.same(b, "B clone and dist")
    t.eq(t.package(B), b_entry, "B state entry")
    t.gitpm_ok("enable", A)
    t.eq((t.dist(A) / "index.js").read_text(), f'export const greeting = "{A} 0.2.0";\n', "enable deploys 0.2.0")
