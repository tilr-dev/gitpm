# Tasks

## 1. Implementation

- [x] 1.1 Read curl's stdout incrementally in `exec::http_get` and stop (kill curl) after 16 MiB + status trailer; verify `exec::oversized_streamed_catalog_response_is_refused` (named like the Dagger case `uc_065:oversized_streamed_catalog_response_is_refused`): a loopback server streams 50 MiB chunked without Content-Length and the request fails with `http`
- [x] 1.2 Keep existing behavior for 200/404/3xx and small bodies; verify `exec::http_get_rejects_non_http_responses` and catalog HTTP tests still pass
- [x] 1.3 Note the enforced limit in README "Rules of the library"/catalog transport and run fmt, clippy and `cargo test --workspace`
- [x] 1.4 Remove `xfail="catalog-streaming-size-limit"` from `uc_065:oversized_streamed_catalog_response_is_refused` (it then passes); verify `./itests/dt --only scenarios --filter uc_065` (TILR-12)

## Regression evidence

- Loopback chunked-response regression fails with the former unbounded `Command::output` reader and passes with the bounded reader. The test omits curl `--max-filesize` to reproduce old curl behavior independently of the installed version.
- Boundary coverage: empty/small bodies, exactly 16 MiB, 16 MiB + 1–5 bytes, 404 and 301.
- `uc_065`: 18 other scenarios passed; the streamed-body scenario passed after correcting the new assertion call.
