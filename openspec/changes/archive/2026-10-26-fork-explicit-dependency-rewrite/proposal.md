## Why
Fork currently rewrites internal dependencies implicitly. Authors need to choose when a fork starts depending on its sibling forks.

## What Changes
- BREAKING: fork preserves dependency IDs and requirements; fork results no longer include dependency rewrites.
- Add an explicit library function, FFI function and `fork-rewrite DIR` command that rewrite dependencies using the local fork manifests.
- Print the explicit rewrite command before release in fork next steps.

## Capabilities
### Modified Capabilities
- multi-package-repos: preserve dependencies during fork and rewrite explicitly.
- fork-replacement: separate rewrite results and next steps.
- cli: expose fork-rewrite.
- ffi-bindings: expose forkRewrite without a workspace.
- plugin-metadata: preserve metadata during the explicit rewrite.

## Impact
Library, CLI and UniFFI fork surfaces, repository manifests and user documentation.
