## Why
Packages need to deploy their repository root or an arbitrary output folder and describe required builds without executing them.

## What Changes
- Add dist, buildCommand and buildRequired manifest fields with existing dist behavior as the default.
- Deploy and hash the declared committed folder; explain missing build output.

## Impact
Manifest parsing, package validation, Git exports, catalog hashes and integration coverage.
