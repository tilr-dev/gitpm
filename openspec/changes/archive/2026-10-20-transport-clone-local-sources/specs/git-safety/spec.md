## ADDED Requirements

### Requirement: Local source clones use Git transport
Registration, catalog source discovery and package installation SHALL clone local sources through Git transport rather than copying their object directories. Transient server-info files disappearing while the source is read SHALL NOT prevent reading committed releases. Clones SHALL have their own objects and retain the original source as origin.

#### Scenario: Transient server-info file disappears
- **GIVEN** a local source with a committed tagged release and a transient `objects/info/packs_*` file removed when a filesystem copier opens it
- **WHEN** the source is registered, discovered by catalog update, or installed
- **THEN** the operation succeeds with the committed release and does not copy the transient file
- **AND** installation records the dependency edges as usual
