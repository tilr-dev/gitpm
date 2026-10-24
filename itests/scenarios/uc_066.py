"""UC-066: Fork replacement main scenario, happy path (P0, mandatory).

Each step starts from the fixture that ends with the previous step (g6_base →
g6_base_catalog → g6_base_installed → g6_forked → g6_replaced), so a failing step does not
hide the others; `full_journey` runs all eight steps in one workspace from authoring on.
"""

import hashlib
import json

from gitpmtest import T, case

ORIGINAL, FORK = "vendor/base", "mine/base"
CONSUMERS = ["app/a", "app/b"]
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


def sha(path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def providers(t: T) -> dict:
    view = resolved(t)
    return {c: next(r for r in view[c]["dependencies"] if r["id"] == ORIGINAL)["provider"] for c in CONSUMERS}


def provider_payload(t: T) -> str:
    """Hash of the payload of the package that provides vendor/base to the consumers."""
    (provider,) = set(providers(t).values())
    return sha(t.dist(provider) / "index.js")


def step1_publish(t: T):
    for id in [ORIGINAL, *CONSUMERS]:
        t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@1.0.0")
    for id, deps in [(ORIGINAL, {}), ("app/a", {ORIGINAL: "^1.0"}), ("app/b", {ORIGINAL: "^1.0"})]:
        t.eq(json.loads(t.catalog_release(id, "1.0.0").read_text())["manifest"].get("dependencies", {}), deps,
             f"{id} catalog dependencies")


def step2_install(t: T):
    for id in CONSUMERS:
        t.gitpm_ok("install-catalog", id)
    t.eq(sorted(t.state()["packages"]), ["app/a", "app/b", ORIGINAL], "vendor/base pulled in as a dependency")
    view = resolved(t)
    for c in CONSUMERS:
        t.eq(t.state()["packages"][c]["dependencies"], {ORIGINAL: "^1.0"}, f"{c} edge")
    t.eq(view[ORIGINAL].get("dependents"), CONSUMERS, "vendor/base dependents")
    t.eq(providers(t), {c: ORIGINAL for c in CONSUMERS}, "providers")


def step3_fork(t: T, fork):
    t.gitpm_ok("fork", ORIGINAL, "--ns", "mine", "--into", fork)
    t.write(fork, {"dist/index.js": FORK_PAYLOAD})
    t.gitpm_ok("commit", fork, "1.0.1")
    manifest = json.loads((fork / "plugin.json").read_text())
    t.eq(manifest["replaces"], {"id": ORIGINAL, "version": "1.0.0"}, "fork declares replaces 1.0.0")
    t.eq(manifest["version"], "1.0.1", "fork version")
    t.gitpm_ok("install", fork)
    t.eq(t.package(FORK)["current"]["version"], "1.0.1", "fork installed")
    t.eq(providers(t), {c: ORIGINAL for c in CONSUMERS}, "installing the fork redirects nothing")


def step4_replace(t: T):
    r = t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    t.lacks(r.err, "Warning", "[y/N]")
    t.eq(t.state()["replacements"], {ORIGINAL: FORK}, "binding")
    t.eq(providers(t), {c: FORK for c in CONSUMERS}, "providers")
    t.eq(resolved(t)[FORK].get("replacing"), [ORIGINAL], "list: replacing")
    t.has(r.out, f"{ORIGINAL} replaced by {FORK} 1.0.1 (as {ORIGINAL} 1.0.0) for: app/a, app/b")
    t.has(t.gitpm_ok("check").out, f"Replacement: {ORIGINAL} → {FORK} (for every consumer)")
    t.eq(provider_payload(t), hashlib.sha256(FORK_PAYLOAD.encode()).hexdigest(), "consumers get the fork payload")
    t.ok(not t.dist(ORIGINAL).exists(), "dist/ holds only the active provider, not the replaced original")


def step5_reopen(t: T):
    before = (t.ws / "state.json").read_bytes()
    t.eq(providers(t), {c: FORK for c in CONSUMERS}, "providers in a new process")
    t.eq(t.state()["replacements"], {ORIGINAL: FORK}, "binding in a new process")
    t.eq((t.ws / "state.json").read_bytes(), before, "state.json untouched by reading")


def step6_fork_update(t: T, fork):
    t.write(fork, {"dist/index.js": "fork payload 1.0.2\n"})
    t.gitpm_ok("commit", fork, "1.0.2")
    t.gitpm_ok("update", FORK)
    t.eq(t.package(FORK)["current"]["version"], "1.0.2", "fork version")
    t.eq(t.rev(t.clone(FORK)), t.rev(fork, f"{FORK}@1.0.2"), "fork clone HEAD")
    t.eq(providers(t), {c: FORK for c in CONSUMERS}, "providers")
    t.eq(provider_payload(t), hashlib.sha256(b"fork payload 1.0.2\n").hexdigest(), "consumers follow the fork")


def step7_switch_back(t: T, fork):
    fork_head = t.rev(t.clone(FORK))
    fork_author = t.snap(fork, git=True)
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.ok("replacements" not in t.state(), "binding removed")
    t.eq(providers(t), {c: ORIGINAL for c in CONSUMERS}, "providers")
    t.eq(provider_payload(t), sha(t.dist(ORIGINAL) / "index.js"), "original payload")
    t.has((t.dist(ORIGINAL) / "index.js").read_text(), "vendor/base 1.0.0")  # redeployed by --back
    t.eq(t.package(FORK)["current"]["version"], "1.0.2", "fork still installed")
    t.eq(t.rev(t.clone(FORK)), fork_head, "fork history kept")
    t.eq((t.dist(FORK) / "index.js").read_text(), "fork payload 1.0.2\n", "fork edits kept")
    t.same(fork_author, "fork author repository")


def step8_uninstall_fork(t: T):
    t.ok(not resolved(t)[FORK].get("dependents"), "fork has no dependents")
    t.gitpm_ok("uninstall", FORK)
    t.not_installed(FORK)
    t.eq(providers(t), {c: ORIGINAL for c in CONSUMERS}, "providers")


@case(fixture="g6_base", spec=["catalog/register-one-version"])
def s1_publish_to_catalog(t: T):
    """1. vendor/base, app/a, app/b (^1.0) released in Git and registered in a local catalog."""
    step1_publish(t)


@case(fixture="g6_base_catalog", spec=["dependencies/install-resolves-dependencies#dependency-pulled-from-the-catalog"])
def s2_install_pulls_dependency(t: T):
    """2. install-catalog app/a and app/b; vendor/base is pulled in; list shows the edges."""
    step2_install(t)


@case(fixture="g6_base_installed", spec=["fork-replacement/fork-copies-an-installed-package", "fork-replacement/fork-metadata#fork-manifest", "authoring/commit-releases-a-version"])
def s3_fork_edit_release_install(t: T):
    """3. fork vendor/base as mine/base, edit payload, release 1.0.1 (replaces vendor/base 1.0.0), install."""
    step3_fork(t, t.repo(FORK))


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global#replace-for-all-consumers", "dist/replace-removes-the-original-s-dist"])
def s4_replace_without_prompt(t: T):
    """4. replace: no prompt; list/check show the binding; consumers get the fork payload."""
    step4_replace(t)


@case(fixture="g6_replaced", spec=["fork-replacement/replacement-is-explicit-and-global"])
def s5_reopen_keeps_binding(t: T):
    """5. a new process sees the same binding and resolution."""
    step5_reopen(t)


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules"])
def s6_fork_update_followed(t: T):
    """6. release mine/base 1.0.2 and update it: both consumers follow the new fork revision."""
    step6_fork_update(t, t.repo(FORK))


@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "dist/switching-back-redeploys-the-original#back-to-the-original"])
def s7_switch_back(t: T):
    """7. switch back: consumers on the original payload; the fork stays with history and edits."""
    step6_fork_update(t, t.repo(FORK))
    step7_switch_back(t, t.repo(FORK))


@case(fixture="g6_replaced", spec=["install-uninstall/only-manager-owned-files-are-deleted#uninstalling-a-fork"])
def s8_uninstall_clean_fork(t: T):
    """8. after switching back the clean fork uninstalls; no dependents."""
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    step8_uninstall_fork(t)


@case(fixture="g6_base", spec=["fork-replacement/replacement-is-explicit-and-global", "fork-replacement/switching-back"])
def full_journey(t: T):
    """Steps 1–8 in one workspace, each with exit 0 and payload hashes of the active provider."""
    fork = t.root / "authors/mine-base"
    step1_publish(t)
    step2_install(t)
    step3_fork(t, fork)
    step4_replace(t)
    step5_reopen(t)
    step6_fork_update(t, fork)
    step7_switch_back(t, fork)
    step8_uninstall_fork(t)
