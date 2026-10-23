## ADDED Requirements

### Requirement: Typed release and catalog repository operations
Release discovery, lost-work inspection, catalog generation, checks,
revision resolution, package discovery and authentication support SHALL access
Git through typed transport operations without assembling Git command arguments
in their production callers. Existing results, error kinds and messages,
repository mutations, SSH settings and execution bounds SHALL be preserved.

#### Scenario: Release inspection
- **WHEN** releases are inspected through the typed transport
- **THEN** tags, revision resolution and committed file
  reads have the same results and failure behavior as before

#### Scenario: Catalog and package content
- **WHEN** a catalog or package discovery reads committed trees and manifests
- **THEN** the same paths, raw tree entries and manifest text are returned,
  with Git argument construction confined to the transport

#### Scenario: Local work and origin inspection
- **WHEN** checks inspect repository status, refs, history or origin settings
- **THEN** the same local-work and source information is returned with the same
  SSH policy and errors through typed operations
