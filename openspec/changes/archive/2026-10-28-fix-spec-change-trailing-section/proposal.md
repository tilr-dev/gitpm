# Respect requirement section boundaries in specification authorization

## Why
Adding a requirement before an unchanged trailing section falsely requires modify authorization: the checker assigns the trailing section to the last requirement and mistakes its changed owner for moved requirement content.

## What Changes
- End requirement ownership at headings of the same or higher level.
- Allow purely additive requirements before unchanged trailing sections without a modify grant.
- Preserve authorization for existing requirement edits and transferred scenarios.

## Impact
- Specification change checker and deterministic regression coverage.
