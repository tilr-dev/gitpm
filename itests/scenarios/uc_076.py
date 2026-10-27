"""UC-076: canonical catalog command group, without old aliases."""

import json

from gitpmtest import T, case


@case(fixture="hello", spec=["cli/command-set#catalog-links",
                            "cli/command-set#command-list"])
def catalog_group_runs_every_subcommand(t: T):
    """The four two-word commands reach the existing catalog operations."""
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.2.0")
    source = t.repo("3dce1568-569c-5b53-9321-37185e751947")
    sources = t.root / "group-sources.txt"
    sources.write_text(f"{source}\n")
    update = t.run("gitpm", "catalog", "update", "--sources", sources, check=True)
    t.has(update.out, "Catalog ready: 2 release(s) checked from sources.")
    release = t.run("gitpm", "catalog", "register", source, "--ref", "v0.2.0", check=True)
    t.eq(json.loads(release.out)["manifest"]["id"], "3dce1568-569c-5b53-9321-37185e751947", "registered package")
    show = t.run("gitpm", "catalog", "show", check=True)
    t.has(show.out, "3dce1568-569c-5b53-9321-37185e751947", "latest: 0.2.0")
    links = t.run("gitpm", "catalog", "links", check=True)
    t.eq(json.loads(links.out)["schema"], 1, "catalog links")
    t.ok(not t.ws.exists(), "catalog operations create no workspace")


@case(fixture="hello", spec=["cli/command-set#command-list",
                            "cli/command-set#removed-command"])
def catalog_errors_create_nothing(t: T):
    """Missing/unknown subcommands and all removed aliases fail without changes."""
    before = t.snap(t.root, git=True)
    cases = [
        (["catalog"], "catalog needs a subcommand: show, links, update or register; use gitpm catalog --help"),
        (["catalog", "publish"], "unknown catalog command: publish; use gitpm catalog --help"),
    ] + [([old], f"unknown command: {old}; use --help")
         for old in ("info", "catalog-update", "register")]
    for args, message in cases:
        result = t.run("gitpm", *args)
        t.eq(result.code, 1, f"exit for {args}")
        t.eq(result.err, f"Error: {message}\n", "usage error")
        t.eq(result.out, "", "stdout")
        t.same(before, "fixture")
        t.ok(not t.ws.exists() and t.catalog_empty(), "nothing created")


@case(spec=["cli/help#command-help"])
def catalog_group_help(t: T):
    """Group help lists four operations; subcommand help shows one block."""
    group = t.run("gitpm", "catalog", "--help", check=True).out
    for subcommand in ("show", "links", "update", "register"):
        single = t.run("gitpm", "catalog", subcommand, "--help", check=True).out
        t.has(group, single.strip())
        t.ok(single.startswith(f"  catalog {subcommand} "), "subcommand usage")
        t.eq(sum(line.startswith("  catalog ") for line in single.splitlines()), 1, "one block")
