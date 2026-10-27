## ADDED Requirements

### Requirement: Specification authorization respects requirement boundaries
The specification change checker SHALL end a requirement's ownership at a Markdown heading of the same or higher level. A trailing section outside requirements SHALL NOT be attributed to the preceding requirement.

#### Scenario: Add a requirement before an unchanged trailing section
- **GIVEN** an existing requirement followed by a `## Notes` section
- **WHEN** a new requirement is inserted before that unchanged section without changing existing requirement content
- **THEN** the checker reports no uncovered modification of the existing requirement

#### Scenario: Higher and same-level headings end ownership
- **WHEN** a heading of level one, two or three follows a requirement
- **THEN** the heading and following text are outside that requirement
- **AND** a subsequent requirement heading starts a new owner

#### Scenario: Existing scenarios remain protected
- **WHEN** a new requirement takes a scenario from an existing requirement
- **THEN** the checker still requires modify authorization for the existing requirement
