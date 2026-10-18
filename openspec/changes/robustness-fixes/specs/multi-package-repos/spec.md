## MODIFIED Requirements

### Requirement: Adding packages to an existing fork
`fork … --into DIR` where `DIR` has `gitpm.json` SHALL add the package folders (files of the installed revisions, plain files only) at their original folders, in one commit of only those paths; `gitpm.json` and changed manifests MUST have no uncommitted changes, a configured Git identity is required (authoring), and a failed commit SHALL restore the repository. Every refusal SHALL be `invalid_argument` and change nothing.

#### Scenario: Folder or ID already there
- **WHEN** the fork repository already has the folder, or a package whose ID differs only in letter case
- **THEN** the add is refused

#### Scenario: Nesting folders
- **WHEN** an added folder would contain or be contained in another package folder
- **THEN** the add is refused ("package folder … would nest with …")

#### Scenario: Root package
- **WHEN** a forked package is the root package of its repository
- **THEN** the add is refused ("… is the root package of its repository …; fork it into a new folder")

#### Scenario: Add later
- **WHEN** the user runs `gitpm fork tilr/dock --ns me --into forks/me-plugins` on an existing fork
- **THEN** it prints `Packages added to: …` and `me/dock forked from tilr/dock (packages/dock)`
- **AND** this is covered by `packages_are_added_to_an_existing_fork_with_dependencies_rewritten_both_ways` and `adding_to_a_fork_refuses_a_failing_commit_and_a_missing_identity_untouched`

#### Scenario: No configured identity
- **WHEN** the fork repository has no configured Git identity and the user adds a package to it
- **THEN** the add fails with `invalid_argument` before anything is written (authoring, "Git identity is never invented in existing repositories")
