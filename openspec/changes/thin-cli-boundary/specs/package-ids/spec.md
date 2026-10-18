## MODIFIED Requirements

### Requirement: No default namespace
gitpm SHALL NOT derive or default a namespace: every operation that creates a package (`init`) MUST receive a full `NAMESPACE/NAME`, and `fork` MUST receive the target namespace explicitly. The library SHALL refuse a name without a namespace with `invalid_id`. There SHALL be no namespace configuration file or command.

#### Scenario: init without a namespace
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** the library fails with `invalid_id` ("package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello"), the CLI prints `Error kind: invalid_id`, the exit status is 1 and nothing is created
