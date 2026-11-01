# /// script
# requires-python = ">=3.11"
# dependencies = ["PyYAML==6.0.2"]
# ///
"""Check the two-tier contract of .github/workflows/ci.yml and .config/nextest.toml.

Run with uv run scripts/test_ci_workflow.py (CI runs it in the `test` job).
"""
from pathlib import Path
import re
import tomllib
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
NEXTEST = tomllib.loads((ROOT / ".config/nextest.toml").read_text())
REQUIRED = ["fmt + clippy", "test", "msrv (core crate)", "cargo-deny", "gitleaks"]
# The fast profile on pull requests only; every other event gets the full default profile.
TIER = "${{ github.event_name == 'pull_request' && 'pr' || 'default' }}"


def step(job, run_prefix):
    matches = [s for s in job["steps"] if s.get("run", "").startswith(run_prefix)]
    assert len(matches) == 1, (run_prefix, matches)
    return matches[0]


class CiTiers(unittest.TestCase):
    def setUp(self):
        # PyYAML reads the bare key `on` as the boolean True.
        self.on = WORKFLOW[True]
        self.jobs = WORKFLOW["jobs"]

    def test_triggers(self):
        self.assertEqual(set(self.on), {"merge_group", "pull_request", "push",
                                        "schedule", "workflow_dispatch"})
        self.assertEqual(self.on["push"], {"branches": ["release/**"]},
                         "no run on push to main: the merge queue checked that commit")
        self.assertEqual(self.on["pull_request"]["branches"], ["main", "release/**"])
        self.assertEqual(len(self.on["schedule"]), 1)
        self.assertEqual(WORKFLOW["concurrency"],
                         {"group": "ci-${{ github.ref }}", "cancel-in-progress": True})

    def test_required_checks_run_on_every_event(self):
        names = {job["name"]: job for job in self.jobs.values()}
        for name in REQUIRED:
            with self.subTest(check=name):
                self.assertIn(name, names)
                self.assertNotIn("if", names[name], "a required check must report on every event")

    def test_test_job_tiers(self):
        job = self.jobs["test"]
        run = step(job, "cargo nextest run")
        self.assertEqual(run["run"], 'cargo nextest run --workspace --locked --profile "$TIER"')
        self.assertEqual(run["env"]["TIER"], TIER)
        self.assertFalse(any(s.get("run", "").startswith("cargo test --workspace --locked")
                             and "--doc" not in s["run"] for s in job["steps"]))
        doc = step(job, "cargo test --workspace --locked --doc")
        self.assertEqual(doc["if"], "github.event_name != 'pull_request'")

    def test_nextest_is_pinned(self):
        install = next(s for s in self.jobs["test"]["steps"]
                       if s.get("name") == "Install cargo-nextest")
        self.assertRegex(install["env"]["NEXTEST_VERSION"], r"^\d+\.\d+\.\d+$")
        self.assertRegex(install["env"]["NEXTEST_SHA256"], r"^[0-9a-f]{64}$")
        self.assertIn("sha256sum -c -", install["run"])

    def test_profiles(self):
        profiles = NEXTEST.get("profile", {})
        self.assertNotIn("default-filter", profiles.get("default", {}),
                         "the full tier runs every test")
        fast = profiles["pr"]["default-filter"]
        for binary in ("dependencies", "lifecycle", "dist_active", "monorepo"):
            self.assertRegex(fast, rf"binary\({binary}\)")
        self.assertTrue(re.match(r"\s*not\s*\(", fast), "the fast tier only leaves tests out")

    def test_queue_does_not_save_caches(self):
        for job_id in ("lint", "test", "msrv"):
            cache = next(s for s in self.jobs[job_id]["steps"]
                         if s.get("uses", "").startswith("Swatinem/rust-cache@"))
            with self.subTest(job=job_id):
                self.assertEqual(cache["with"]["save-if"],
                                 "${{ github.event_name != 'merge_group' }}")


if __name__ == "__main__":
    unittest.main()
