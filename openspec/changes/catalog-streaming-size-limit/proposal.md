# Proposal: Enforce the catalog size limit on streamed responses

## Why

`exec::http_get` passes `--max-filesize 16777216` to curl, which only refuses a body whose Content-Length is announced; a streamed (chunked) response is read in full. The ATQ case UC-065 and the Dagger run found it: a 50 MiB streamed body passed against the 16 MiB limit. On 2026-10-16 the owner said not to fix it then (Codex session 2026-10-16 15:48; Claude S3 "streamed catalog >16 MiB accepted"). Status: verified unchanged on `dev` (exec.rs uses only `--max-filesize`); spec `catalog` now states the gap explicitly (scenario "Streamed body without a length"). Tracker: TILR-12.

## What Changes

- Cap the bytes read from curl's stdout at 16 MiB regardless of headers (stream the child's stdout and kill curl when the cap is exceeded), reporting `http`.
- Keep the existing timeouts, protocol and redirect rules.

## Capabilities

### Modified Capabilities
- `catalog`: "Catalog locations and transport" — the 16 MiB limit also applies without Content-Length (scenario "Streamed body without a length" changes from "read in full" to `http`).

## Out of scope

- Replacing curl with an embedded HTTP client (see `embedded-git-and-http`, which lands after this change).
- Changing the limit value.

## Impact

`crates/gitpm/src/exec.rs` only; a loopback test server in tests.
