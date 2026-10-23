## MODIFIED Requirements

### Requirement: Typed network transport
The transport module SHALL expose a Transport trait with typed methods for clone, fetch, remote HEAD refresh, remote ref lookup, read-only access probes and HTTP GET. Network call sites in gitpm SHALL use these methods rather than assemble raw network Git commands. The transport API visible to production callers SHALL contain only typed operations; raw Git helpers SHALL remain internal to the transport module. Transport failures SHALL retain the existing gitpm error kinds and messages.

#### Scenario: Clone and fetch through the interface
- **WHEN** gitpm clones a repository or fetches tags, records origin tags or refreshes a remote HEAD
- **THEN** it invokes the typed transport operation with the same options, refspecs and SSH setting as before, with subprocess creation confined to the transport module

#### Scenario: Streamed catalog size bound
- **WHEN** a catalog response has no declared size
- **THEN** the transport reads at most 16 MiB plus the four-byte status trailer plus one overflow byte, kills curl on overflow, and retains the existing 200/404/error semantics
- **AND** both loopback streamed-response tests run inside the transport module

## ADDED Requirements

### Requirement: Typed scaffold repository operations
Scaffold authoring SHALL use typed transport operations without assembling Git command arguments in production callers. Repository initialization, configured identity checks, staging, commits, tags, index location, history and remote configuration SHALL preserve existing results, errors, SSH settings, execution bounds, hooks and signing behavior. All production library callers SHALL use transport facades that select the sole system implementation in one place. Test fixtures, unit and integration tests, and build scripts are outside this production-only boundary.

#### Scenario: Initialize and release a package
- **WHEN** an author initializes a repository or commits and tags a package version
- **THEN** typed operations preserve the initial branch, local demo identity fallback, staged paths, commit messages and tag behavior
- **AND** failed commits restore manifests and the index as before

#### Scenario: Release selected packages
- **WHEN** an author releases only selected package folders or releases multiple packages together
- **THEN** the selected-path commit behavior is preserved and multiple tags are created atomically without overwriting existing tags

#### Scenario: Configure origin without network access
- **WHEN** an author sets a repository's origin
- **THEN** typed operations preserve replacement of all fetch URLs and any configured push URLs without contacting the remote

#### Scenario: Replace the transport implementation
- **WHEN** the transport backend is replaced
- **THEN** one selection point inside the transport module controls all production callers
- **AND** the production boundary test rejects raw Git helpers and direct system backend access outside the transport
