# Evidence and scope (2026-10-20)

## Gitleaks
`bin/check gitpm:gitleaks gitpm:deny` fails on the unmodified branch (5a91935).
`gitleaks git --redact --verbose --report-format json` confirms exactly one
generic-api-key finding: fingerprint
`bbe8cf6511b9cb96d858f7c65f046e0c06dc9295:itests/scenarios/uc_049.py:generic-api-key:25`.
The file creates its own authenticated HTTP Git server on `127.0.0.1` with an
ephemeral port. `setup` supplies the constant to that server, `helper` supplies
it to the client, and the revocation cases change the server password. This is
a synthetic test fixture, not a credential for an external service.

Changing today's value would leave the historical finding. A fingerprint
exception fixes the history scan without excluding a file or rule. No secret
value needs to be copied into the exception or this document.

## Cargo-deny
The baseline rejects RUSTSEC-2026-0374. `Cargo.lock` records
uniffi/uniffi_bindgen 0.32.2 -> askama/askama_derive 0.16.1 -> basic-toml 0.1.10.
On 2026-10-20, the crates.io API reports 0.32.2 and 0.16.1 as the latest stable
releases; askama_derive 0.16.1 still requires basic-toml ^0.1.1. No released
upgrade removes it. The advisory is informational (unmaintained), with no
patched version; it is not a reported vulnerability. It affects the parser
used by Askama's compile-time template configuration.

Sources and recheck commands:
- `https://crates.io/api/v1/crates/uniffi`
- `https://crates.io/api/v1/crates/askama_derive`
- `https://crates.io/api/v1/crates/askama_derive/0.16.1/dependencies`
- `https://rustsec.org/advisories/RUSTSEC-2026-0374.html`
- `https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/basic-toml/RUSTSEC-2026-0374.md`
- Recheck the API endpoints with `curl -fsSL`, then `cargo tree -i basic-toml`.
  Update upstream dependencies and remove the exception once basic-toml is gone.

## Validation
Both existing checks fail before the configuration fix and must pass after it.
The full default `bin/check` suite verifies the rest of the workspace. No product
code changes or new runtime tests are needed for these scoped configuration fixes.
