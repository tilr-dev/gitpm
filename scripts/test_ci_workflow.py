# /// script
# requires-python = ">=3.11"
# dependencies = ["PyYAML==6.0.2"]
# ///
"""Check the two-tier contract of .github/workflows/ci.yml and .config/nextest.toml.

Run with uv run scripts/test_ci_workflow.py (CI runs it in the `test` job).
"""
from pathlib import Path
import json
import re
import tomllib
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
NEXTEST = tomllib.loads((ROOT / ".config/nextest.toml").read_text())
REQUIRED = ["fmt + clippy", "test", "msrv (core crate)", "cargo-deny", "gitleaks"]
# The fast profile only on pull requests into main (the merge queue then runs every test);
# release branches have no merge queue, so their pull requests and every other event run all.
TIER = ("${{ github.event_name == 'pull_request' && github.base_ref == 'main' "
        "&& 'pr' || 'default' }}")


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
        self.assertIn("edited", self.on["pull_request"]["types"],
                      "a PR retargeted to release/** reruns with the full tier")
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
        self.assertEqual(job["env"]["TIER"], TIER)
        self.assertNotIn("env", run)
        self.assertFalse(any(s.get("run", "").startswith("cargo test --workspace --locked")
                             and "--doc" not in s["run"] for s in job["steps"]))
        doc = step(job, "cargo test --workspace --locked --doc")
        self.assertEqual(doc["if"], "env.TIER == 'default'")

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


def event_condition(expression, event, ref):
    """Evaluate the small Actions expression subset used by job/cache routing."""
    expression = expression.removeprefix("${{").removesuffix("}}")
    expression = expression.replace("github.event_name", repr(event))
    expression = expression.replace("github.ref", repr(ref))
    expression = expression.replace("&&", " and ").replace("||", " or ")
    return eval(expression, {"__builtins__": {}},
                {"startsWith": str.startswith, "success": lambda: True})


class DaggerCi(unittest.TestCase):
    def setUp(self):
        self.job = WORKFLOW["jobs"]["itests"]
        self.steps = self.job["steps"]

    def named(self, name):
        return next(s for s in self.steps if s.get("name") == name)

    def test_event_routing_and_nightly_cache_ownership(self):
        save_steps = [self.named(name) for name in
                      ("Export stopped Dagger cache", "Save Dagger cache")]
        cases = [
            ("schedule", "refs/heads/main", True, True),
            ("schedule", "refs/heads/release/1.2", False, False),
            ("workflow_dispatch", "refs/heads/main", True, False),
            ("workflow_dispatch", "refs/heads/task/example", True, False),
            ("push", "refs/heads/release/1.2", True, False),
            ("push", "refs/heads/main", False, False),
            ("push", "refs/heads/task/example", False, False),
            ("pull_request", "refs/pull/1/merge", False, False),
            ("pull_request", "refs/heads/release/1.2", False, False),
            ("merge_group", "refs/heads/gh-readonly-queue/main/example", False, False),
        ]
        for event, ref, run, save in cases:
            with self.subTest(event=event, ref=ref):
                self.assertEqual(event_condition(self.job["if"], event, ref), run)
                for step in save_steps:
                    self.assertEqual(event_condition(step["if"], event, ref), save)
                    self.assertIn("success()", step["if"])
        self.assertEqual(self.job["name"], "itests")
        self.assertNotIn(self.job["name"], REQUIRED)
        self.assertEqual(self.job["runs-on"], "ubuntu-latest")
        self.assertEqual(self.job["permissions"], {"contents": "read"})
        self.assertFalse(self.steps[0]["with"]["persist-credentials"])
        self.assertFalse(any("continue-on-error" in s for s in self.steps))
        self.assertNotIn("continue-on-error", self.job)

    def test_cli_matches_module_and_checks_download(self):
        env = self.job["env"]
        engine = json.loads((ROOT / "itests/dagger.json").read_text())["engineVersion"]
        self.assertEqual("v" + env["DAGGER_VERSION"], engine)
        self.assertIn("DAGGER_VERSION=" + engine, (ROOT / "itests/dt").read_text())
        self.assertRegex(env["DAGGER_SHA256"], r"^[0-9a-f]{64}$")
        install = self.named("Install the Dagger CLI")["run"]
        self.assertIn("dagger_v${DAGGER_VERSION}_linux_amd64.tar.gz", install)
        self.assertIn('${DAGGER_SHA256}  $RUNNER_TEMP/dagger.tgz', install)
        self.assertLess(install.index("sha256sum -c -"), install.index("tar -xzf"))
        start = self.named("Start local Dagger engine")["run"]
        self.assertIn('registry.dagger.io/engine:v${DAGGER_VERSION}', start)
        self.assertIn('--name gitpm-itests --privileged', start)
        self.assertEqual(env["_EXPERIMENTAL_DAGGER_RUNNER_HOST"],
                         "docker-container://gitpm-itests")

    def test_complete_suite_is_bounded_and_reports_survive_failure(self):
        self.assertEqual(self.job["env"]["DT_ENGINE"], "dagger")
        run = self.named("Run the complete Dagger suite")
        self.assertEqual(run["run"], "env -u SSH_AUTH_SOCK itests/dt --jobs 2")
        module = (ROOT / "itests/src/gitpm_itests/main.py").read_text()
        self.assertIn('.with_env_variable("CARGO_BUILD_JOBS", "2")', module)
        self.assertNotIn("if", run)
        upload = self.named("Upload Dagger results")
        self.assertEqual(upload["if"], "always()")
        self.assertTrue(upload["uses"].startswith("actions/upload-artifact@"))
        self.assertEqual(upload["with"]["path"], "itests/results/")
        self.assertIn("uv==", self.named("Install report tools")["run"])

    def test_engine_cache_round_trip_order_and_keys(self):
        restore = self.named("Restore Dagger cache")
        save = self.named("Save Dagger cache")
        self.assertTrue(restore["uses"].startswith("actions/cache/restore@"))
        self.assertTrue(save["uses"].startswith("actions/cache/save@"))
        self.assertNotIn("if", restore)
        self.assertEqual(restore["with"]["path"], save["with"]["path"])
        self.assertEqual(save["with"]["key"],
                         "${{ steps.dagger-cache.outputs.cache-primary-key }}")
        self.assertIn("${{ env.DAGGER_VERSION }}", restore["with"]["restore-keys"])
        self.assertTrue(restore["with"]["key"].startswith(
            restore["with"]["restore-keys"].strip()))
        self.assertIn("${{ github.run_id }}", restore["with"]["key"])
        self.assertIn("${{ github.run_attempt }}", restore["with"]["key"])
        start = self.named("Start local Dagger engine")["run"]
        self.assertLess(start.index("tar --numeric-owner"), start.index("docker run"))
        self.assertIn('$RUNNER_TEMP/dagger-state:/var/lib/dagger', start)
        stop = self.named("Stop local Dagger engine")
        self.assertEqual(stop["if"], "always()")
        self.assertIn("docker stop gitpm-itests", stop["run"])
        export = self.named("Export stopped Dagger cache")
        self.assertIn('-C "$RUNNER_TEMP/dagger-state" .', export["run"])
        self.assertIn('chown', export["run"])
        ordered = [restore, self.named("Start local Dagger engine"),
                   self.named("Run the complete Dagger suite"), stop, export, save]
        indices = [self.steps.index(s) for s in ordered]
        self.assertEqual(indices, sorted(indices))


if __name__ == "__main__":
    unittest.main()
