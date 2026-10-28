# /// script
# requires-python = ">=3.10"
# dependencies = ["PyYAML==6.0.2"]
# ///
"""Exercise event routing in one or more specification authorization workflows.

Run with uv run scripts/test_spec_change_workflow.py [workflow.yml ...].
"""
import copy
import os
from pathlib import Path
import subprocess
import sys
import unittest

import yaml

WORKFLOWS = [Path(arg) for arg in sys.argv[1:]] or [
    Path(__file__).resolve().parents[1] / ".github/workflows/spec-change.yml"
]
PR_STEPS = [
    "Fetch specification trees without a checkout",
    "Download the content-pinned shared checker",
    "Authorize specification changes",
]


def selected_steps(workflow, event):
    """Evaluate the deliberately small event-only condition contract."""
    job = workflow["jobs"]["spec-change"]
    if "if" in job:
        raise AssertionError("the required job must run for both events")
    selected = []
    for step in job["steps"]:
        condition = step.get("if")
        if condition is None:
            selected.append(step)
        elif condition == f"github.event_name == '{event}'":
            selected.append(step)
        elif condition not in {
            "github.event_name == 'merge_group'",
            "github.event_name == 'pull_request_target'",
        }:
            raise AssertionError(f"unsupported event condition: {condition}")
    return selected


def check_routing(workflow):
    triggers = workflow["on"]
    assert triggers["merge_group"]["types"] == ["checks_requested"]
    assert triggers["pull_request_target"]["branches"] == ["main"]
    assert workflow["jobs"]["spec-change"]["name"] == "spec-change"
    assert "env" not in workflow
    job = workflow["jobs"]["spec-change"]
    assert "env" not in job
    pr_steps = selected_steps(workflow, "pull_request_target")
    assert [step["name"] for step in pr_steps] == PR_STEPS
    assert "spec_merge.py" in pr_steps[0]["run"]
    assert "refs/inspection/merge" in pr_steps[2]["run"]
    assert "ITSAPLAN_READ_ONLY_TOKEN" in pr_steps[2]["env"]
    queue_steps = selected_steps(workflow, "merge_group")
    assert len(queue_steps) == 1
    queue = queue_steps[0]
    assert "env" not in queue and "uses" not in queue
    assert "${{" not in queue["run"]
    assert not selected_steps(workflow, "push")
    return queue


class WorkflowEvents(unittest.TestCase):
    def test_queue_runs_without_pr_payload_or_credentials(self):
        for path in WORKFLOWS:
            with self.subTest(workflow=str(path)):
                # BaseLoader keeps the YAML key "on" a string.
                workflow = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
                queue = check_routing(workflow)
                result = subprocess.run(
                    ["bash", "--noprofile", "--norc", "-euo", "pipefail", "-c", queue["run"]],
                    env={"PATH": os.defpath}, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("already passed specification authorization", result.stdout)

    def test_missing_trigger_or_step_guard_breaks_contract(self):
        for path in WORKFLOWS:
            workflow = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            mutations = []
            missing_trigger = copy.deepcopy(workflow)
            del missing_trigger["on"]["merge_group"]
            mutations.append(missing_trigger)
            for index in range(len(workflow["jobs"]["spec-change"]["steps"])):
                missing_guard = copy.deepcopy(workflow)
                del missing_guard["jobs"]["spec-change"]["steps"][index]["if"]
                mutations.append(missing_guard)
            for index, mutated in enumerate(mutations):
                with self.subTest(workflow=str(path), mutation=index):
                    with self.assertRaises((AssertionError, KeyError)):
                        check_routing(mutated)


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]])
