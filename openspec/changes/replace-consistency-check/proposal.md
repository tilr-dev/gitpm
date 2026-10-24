# Replacement consistency checks

## Why
Fork metadata can add dependencies or break consumer version requirements. A replacement must preserve consistency of the provider dependency closure and consumers of the replaced original, while reporting unrelated existing breakage, with dependency installation under explicit caller control.

## What Changes
- Add dry-run replacement checks and explicit missing-dependency resolution across library, CLI and FFI.
- Refuse inconsistent replacements without mutations, confirmation or force.
- Return to the original through ordinary replace with the original as provider; remove the separate switch-back operation.
- Update the authorized replacement, dependency, dist, CLI and FFI contracts.

## Impact
Breaking replacement API and CLI changes. Existing install/update and removal force rules remain in their respective capabilities.
