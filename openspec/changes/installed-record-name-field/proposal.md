## Why
Installed package records intentionally store the installed manifest name, but the workspace record field list and the rollback scenario's exact-key assertion omit it. The stale assertion rejects valid state after two updates.

## What Changes
- Include required `name` in the installed package record field list, matching the installed package name snapshot contract.
- Update the rollback scenario's exact-key assertion to include `name` while continuing to reject extra history fields.

## Capabilities
### New Capabilities
- None.
### Modified Capabilities
- workspace-layout: include the installed name in the managed record field list.

## Impact
Specification and integration scenario only; state serialization and runtime behavior already satisfy the required name snapshot contract.
