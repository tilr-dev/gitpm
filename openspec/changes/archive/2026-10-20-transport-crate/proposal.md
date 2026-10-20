# Extract the transport crate

## Why
The git/curl process boundary must be replaceable independently of gitpm policy and workflows.

## What Changes
- Move the existing process boundary and its safety tests into gitpm-transport.
- Define a Transport trait with typed clone, fetch, remote lookup and HTTP operations; keep raw local Git operations available for the following migration tasks.
- Preserve subprocess policies, limits, diagnostics and error kinds.

## Impact
- A new workspace crate; gitpm converts transport errors to its stable public errors.
- git-safety names the crate boundary and relocated tests. Other active changes are untouched.
