## 1. Workspace isolation

- [x] 1.1 Specify explicit workspace selection and isolation.
- [x] 1.2 Audit public library, FFI and CLI entry points for workspace selection and shared mutable state; correct any violations.
- [x] 1.3 Verify install and removal across two roots preserve the other workspace's state and files, including simultaneously open library managers.
- [x] 1.4 Exercise every existing public CLI command with an explicit workspace root or repository path and verify selection is independent of the process working directory.
- [ ] 1.5 Review the changes and run the required checks.
