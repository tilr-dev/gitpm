# Tasks

## 1. Implementation

- [ ] 1.1 Read curl's stdout incrementally in `exec::http_get` and stop (kill curl) after 16 MiB + status trailer; verify with a unit test using a loopback server that streams 50 MiB chunked and expects `http`
- [ ] 1.2 Keep existing behavior for 200/404/3xx and small bodies; verify `exec::http_get_rejects_non_http_responses` and catalog HTTP tests still pass
- [ ] 1.3 Note the enforced limit in README "Rules of the library"/catalog transport and run fmt, clippy and `cargo test --workspace`
- [ ] 1.4 Remove `xfail="catalog-streaming-size-limit"` from `uc_065:oversized_streamed_catalog_response_is_refused` (it then passes); verify `./itests/dt --only scenarios --filter uc_065` (TILR-12)
