# Isolate the transport module

## Why
The git/curl process boundary must be replaceable independently of gitpm policy and workflows.

## What Changes
- Move the existing process boundary and its safety tests into an internal transport module.
- Define a Transport trait with typed clone, fetch, remote lookup and HTTP operations; keep raw local Git operations available for the following migration tasks.
- Preserve subprocess policies, limits, diagnostics and error kinds.

## Impact
- An internal module behind Transport/SystemTransport; no additional published crate (Dan #4414 supersedes the original crate plan).
- git-safety names the module boundary and relocated tests. Other active changes are untouched.
