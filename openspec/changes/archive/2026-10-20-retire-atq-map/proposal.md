# Retire the ATQ coverage map

## Why

TILR-174 records Dan's 2026-10-20 decision: OpenSpec is the source of requirements and scenarios; a separate hand-maintained ATQ map must be retired. The historical queue contains 314 checks, some of whose original references predate subsequent specification changes.

## What Changes

- Reconcile all 314 historical checks with current requirements and scenarios in the matching capabilities.
- Preserve the reconciliation and supersession decisions in this change's design.
- Add any missing observable contract to the appropriate capability.
- Remove the historical hand-maintained map and any repository references to it.

## Non-goals

- Changing CLI, library, FFI, or file-format behavior.
- Implementing other active changes or reviving deliberately removed stand features.
- Changing the generated coverage reports (TILR-173).

## Impact

Documentation and specification consolidation only. Existing integration and cargo tests continue to verify their respective contracts.
