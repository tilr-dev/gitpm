## 1. Specification
- [x] 1.1 Describe fork preparation and explicit release/install/replace; remove composed operation requirements.
- [x] 1.2 Update the pending source-fork change to omit removed options.

## 2. Implementation
- [x] 2.1 Remove composed fork methods, outcomes and resume paths from the library and FFI.
- [x] 2.2 Remove fork-only CLI options and present explicit next steps.
- [x] 2.3 Update documentation and callers for explicit operations.

## 3. Validation
- [x] 3.1 Cover inherited tags and unchanged workspace state for ordinary fork through CLI and library.
- [x] 3.2 Cover explicit installation/replacement of one fork package without installing its sibling.
- [x] 3.3 Cover rejection of removed options without side effects and explicit commit tag conflicts.
- [x] 3.4 Validate the specification and run targeted component checks.
