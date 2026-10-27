"""python3 -m gitpmtest list | fixture MODULE:NAME | case MODULE_SHORT:FUNCTION"""

import importlib
import inspect
import re
import json
import pkgutil
import shutil
import sys
import traceback
from pathlib import Path

import gitpmtest as pmtest
from gitpmtest import specs as specmod

ROOT = Path(__file__).resolve().parents[1]


def import_all():
    for package in ("fixtures", "scenarios"):
        for info in sorted(pkgutil.iter_modules([str(ROOT / package)]), key=lambda i: i.name):
            importlib.import_module(f"{package}.{info.name}")


# The stand CLI and its environment are gone: cases drive only gitpm (design D4).
LEGACY = [
    (re.compile(r"\bPM_[A-Z][A-Z_]*"), "a PM_* variable"),
    (re.compile(r"""(?<![\w-])["']pm(?:-api)?["']"""), "the stand binary pm"),
    (re.compile(r"\.(?:pm_ok|pm_err|pm)\("), "T.pm* (use T.gitpm*)"),
]


def legacy_refs(fn) -> list[str]:
    try:
        text = inspect.getsource(fn)
    except (OSError, TypeError):
        return []
    return [what for rx, what in LEGACY if rx.search(text)]


def module_text(module: str) -> str:
    try:
        return inspect.getsource(sys.modules[module])
    except (OSError, TypeError, KeyError):
        return ""


def registry() -> dict:
    import_all()
    fixtures = pmtest.FIXTURES
    errors = []
    for f in fixtures.values():
        errors += [f"fixture {f.name}: unknown parent {p}" for p in f.requires if p not in fixtures]
        if not f.gitpm and any(fixtures[p].gitpm for p in f.requires if p in fixtures):
            errors.append(f"fixture {f.name}: source tier cannot build on a gitpm-tier parent")
        if f.check is None:
            errors.append(f"fixture {f.name}: no @{f.name}.check — every fixture verifies its state")
    try:
        specs = specmod.load()
    except FileNotFoundError as e:
        specs = None
        errors.append(str(e))
    for f in fixtures.values():
        for what in legacy_refs(f.build) + (legacy_refs(f.check) if f.check else []):
            errors.append(f"fixture {f.name}: references {what}")
    for c in pmtest.CASES:
        if c.fixture and c.fixture not in fixtures:
            errors.append(f"case {c.id}: unknown fixture {c.fixture}")
        for what in legacy_refs(c.fn):
            errors.append(f"case {c.id}: references {what}")
        if not c.spec:
            c.spec = list(getattr(sys.modules[c.module], "SPEC", []))
        if not c.spec:
            errors.append(f"case {c.id}: no spec tag (spec=[...] or module SPEC)")
        if specs is not None:
            for tag in c.spec:
                reason = specmod.check_tag(tag, specs)
                if reason:
                    errors.append(f"case {c.id}: unknown spec tag {tag} ({reason})")
    for module in sorted({c.module for c in pmtest.CASES} | {f.module for f in fixtures.values()}):
        for rx, what in LEGACY:
            if rx.search(module_text(module)):
                errors.append(f"module {module}: references {what}")

    def visit(name, path):
        if name in path:
            errors.append("fixture cycle: " + " -> ".join(path + [name]))
            return
        for p in fixtures.get(name).requires if name in fixtures else []:
            visit(p, path + [name])
    for name in fixtures:
        visit(name, [])
    ids = [c.id for c in pmtest.CASES]
    errors += [f"duplicate case {i}" for i in sorted({i for i in ids if ids.count(i) > 1})]
    if errors:
        sys.exit("gitpmtest registry errors:\n  " + "\n  ".join(errors))
    return {
        "fixtures": {f.name: {"module": f.module, "requires": f.requires, "gitpm": f.gitpm, "doc": f.doc}
                     for f in fixtures.values()},
        "cases": [{"id": c.id, "module": c.module, "fixture": c.fixture, "title": c.title, "spec": c.spec,
                   "xfail": c.xfail} for c in pmtest.CASES],
    }


def materialize(root: Path):
    """Copy the snapshot into the container's writable layer. Files of a lower overlayfs
    layer cannot be renamed (EXDEV) or hard-linked across layers, which pm and git do."""
    if not root.exists() or not any(root.iterdir()):
        return
    copy = root.with_name(root.name + ".gitpmt-copy")
    shutil.copytree(root, copy, symlinks=True)
    for child in list(root.iterdir()):
        shutil.rmtree(child) if child.is_dir() and not child.is_symlink() else child.unlink()
    for child in list(copy.iterdir()):
        child.rename(root / child.name)
    copy.rmdir()


def ready_catalog(t) -> None:
    """GITPM_CATALOG points at /fx/catalog, and a configured local catalog location must
    exist (gitpm refuses a missing one with invalid_argument): it starts as an empty
    folder that catalog-update/register fill. A case about a missing catalog removes it
    or passes another --catalog."""
    t.catalog.mkdir(exist_ok=True)


def run(label: str, fn, t) -> int:
    print(f"== {label}")
    try:
        fn(t)
    except pmtest.Blocked as e:
        print(f"\nBLOCKED {label}: {e}")
        return 77
    except Exception as e:
        print(f"\nFAILED {label}: {e}")
        traceback.print_exc(file=sys.stdout)
        return 1
    print(f"PASS {label}")
    return 0


def main(argv: list[str]) -> int:
    if argv[:1] == ["list"]:
        print(json.dumps(registry(), indent=1))
        return 0
    if len(argv) == 2 and argv[0] == "fixture":
        module, name = argv[1].split(":")
        importlib.import_module(module)
        f = pmtest.FIXTURES[name]
        t = pmtest.T()
        t.root.mkdir(exist_ok=True)
        materialize(t.root)
        ready_catalog(t)
        code = run(f"fixture {name}: build", f.build, t)
        if code == 0:
            code = run(f"fixture {name}: check", f.check, t)
        return code
    if len(argv) == 2 and argv[0] == "case":
        short, fn = argv[1].split(":")
        importlib.import_module(f"scenarios.{short}")
        c = next(c for c in pmtest.CASES if c.id == argv[1])
        t = pmtest.T()
        t.root.mkdir(exist_ok=True)
        materialize(t.root)
        ready_catalog(t)
        return run(f"{c.id} {c.title}".strip(), c.fn, t)
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
