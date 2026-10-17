# Tasks

## 0. Design

- [ ] 0.1 Write design.md (policy file format, matching rules, compatibility with 0.5 readers) and spec deltas for `catalog`; drop `skip_specs`

## 1. Implementation

- [ ] 1.1 Enforce the policy in `catalog-update`/`register`; tests: a source outside the allowed pattern is refused with `catalog` and nothing is written
- [ ] 1.2 Expose the result in the overview; tests for `info` output and FFI `PluginSummary`
