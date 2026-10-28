## Implementation
- [x] Define checked-history requirements and event scenarios (OpenSpec strict validation).
- [x] Reproduce unrelated-branch findings using the current workflow command (regression test).
- [x] Restrict the workflow scan to HEAD and align documentation (workflow review).
- [x] Run regression coverage for unrelated branches, ancestor secrets and merge commits (`python3 scripts/test_gitleaks_workflow.py`).
- [x] Validate the specification, workflow and secret scan (repository CI checks).
