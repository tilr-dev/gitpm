## MODIFIED Requirements

### Requirement: Catalog locations and transport
A catalog location SHALL be a local folder, a path to a `*.json` file (normally `catalog.json`), or an HTTP(S) URL. Remote catalogs MUST use HTTPS; `http://` SHALL be accepted only for loopback hosts (`localhost`, loopback IPs, not `localhost.evil.example` or `user@host`). Requests SHALL not follow redirects and SHALL refuse bodies over 16 MiB, whether or not the server sends a Content-Length. HTTP catalogs SHALL be read-only. Verified by `catalog::locations_are_local_paths_or_http_urls`.

#### Scenario: Plain HTTP to a remote host
- **WHEN** the catalog is `http://pkg.example/catalog.json`
- **THEN** every catalog command fails ("catalog URL must use HTTPS (http:// only for localhost)")

#### Scenario: Oversized streamed response
- **WHEN** an HTTPS catalog server streams a 50 MiB body with chunked encoding and no Content-Length
- **THEN** the request fails with `http` once 16 MiB have been received and nothing is parsed or written
