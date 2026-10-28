"""Run the CI gitleaks command against isolated commit graphs.

Run: python3 scripts/test_gitleaks_workflow.py (gitleaks must be installed).
CI uses the same binary installed by the gitleaks job. A synthetic rule keeps
these tests independent of changes to built-in secret detectors.
"""
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/ci.yml").read_text()
JOB = re.search(r"^  secrets:\n(.*?)(?=^  \S|\Z)", WORKFLOW, re.M | re.S).group(1)
COMMAND = re.search(r"^      - run: '(.*)'$", JOB, re.M).group(1)


class GitleaksHistory(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.repo = self.directory / "repo"
        self.repo.mkdir()
        self.env = dict(os.environ)
        for key in list(self.env):
            if key.startswith("GIT_") or key.startswith("GITLEAKS_"):
                del self.env[key]
        self.env.update(
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_SYSTEM=os.devnull,
            GIT_AUTHOR_DATE="2026-01-01T00:00:00Z",
            GIT_COMMITTER_DATE="2026-01-01T00:00:00Z",
        )
        config = self.directory / "gitleaks.toml"
        config.write_text('[[rules]]\nid = "history-fixture"\n'
                          'description = "Synthetic history test marker"\n'
                          'regex = """history-test-marker-[a-z]+"""\n')
        self.env["GITLEAKS_CONFIG"] = str(config)
        self.binary = os.environ.get("GITLEAKS") or shutil.which("gitleaks")
        self.assertTrue(self.binary, "install gitleaks before running this test")
        self.git("init", "-b", "main")
        self.git("config", "user.name", "History Test")
        self.git("config", "user.email", "history@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.commit("clean.txt", "clean\n")
        self.clean = self.git("rev-parse", "HEAD").strip()
        self.git("checkout", "-b", "unmerged")
        self.marker = "history-test-marker-" + "synthetic"
        self.commit("secret.txt", self.marker)
        self.leaked = self.git("rev-parse", "HEAD").strip()
        self.git("update-ref", "refs/remotes/origin/unmerged", self.leaked)
        self.git("checkout", "main")
        self.git("branch", "-D", "unmerged")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, env=self.env,
                              check=True, text=True, capture_output=True).stdout

    def commit(self, name, content):
        (self.repo / name).write_text(content)
        self.git("add", name)
        self.git("commit", "-m", "History fixture")

    def scan(self, expected):
        args = shlex.split(COMMAND)
        self.assertEqual(args[0], "$GITLEAKS")
        result = subprocess.run([self.binary, *args[1:]], cwd=self.repo,
                                env=self.env, text=True, capture_output=True)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expected, output)
        self.assertNotIn(self.marker, output, "findings must be redacted")
        if expected == 1:
            self.assertIn("leaks found: 1", output)

    def test_workflow_contract(self):
        triggers = WORKFLOW.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        for event in ("push", "pull_request", "merge_group"):
            self.assertRegex(triggers, rf"(?m)^  {event}:")
        self.assertNotRegex(JOB, r"(?m)^    if:")
        self.assertRegex(JOB, r"fetch-depth: 0(?:\s|$)")
        self.assertIn("persist-credentials: false", JOB)
        self.assertIn("--redact", shlex.split(COMMAND))

    def test_unrelated_fetched_branch_is_excluded(self):
        for ref in ("refs/heads/main", "refs/pull/1/merge",
                    "refs/heads/gh-readonly-queue/main/test"):
            with self.subTest(ref=ref):
                self.git("update-ref", ref, self.clean)
                self.git("checkout", "--detach", ref)
                self.scan(0)

    def test_deleted_secret_in_ancestor_is_detected(self):
        self.git("checkout", "--detach", self.leaked)
        self.git("rm", "secret.txt")
        self.git("commit", "-m", "Remove marker from tree")
        self.scan(1)

    def test_merge_parent_secret_is_detected(self):
        self.commit("main-only.txt", "diverged\n")
        self.git("merge", "--no-ff", self.leaked, "-m", "Merge fixture")
        merge = self.git("rev-parse", "HEAD").strip()
        for ref in ("refs/pull/1/merge", "refs/heads/gh-readonly-queue/main/test"):
            with self.subTest(ref=ref):
                self.git("update-ref", ref, merge)
                self.git("checkout", "--detach", ref)
                self.scan(1)


if __name__ == "__main__":
    unittest.main()
