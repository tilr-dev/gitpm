## MODIFIED Requirements

### Requirement: init creates a package with its first release
`init NAMESPACE/NAME --into DIR` SHALL scaffold a package with a manifest (`id`, `version` 0.1.0, `"title": "NAME"`), a demo `dist/index.js`, one commit and the tag `NAMESPACE/NAME@0.1.0`. A `DIR` that does not exist SHALL become a single-package repository. `--into` is required and nothing SHALL ever be overwritten. Verified by `scaffold::init_into_a_new_folder_makes_a_tagged_single_package_repository` and `scaffold::init_never_overwrites_and_validates_ids`.

#### Scenario: Scaffold files and commit
- **WHEN** `init acme/hello --into hello` creates a new single-package repository
- **THEN** it also writes `.gitignore` and `README.md`, and the one commit is `Create hello 0.1.0` (a package added to a multi-package repository gets `Create acme/hello 0.1.0`)

#### Scenario: New package repository
- **WHEN** the user runs `gitpm init acme/hello --into hello`
- **THEN** `hello` is a Git repository at tag `acme/hello@0.1.0` and the output contains `Created:`, `ID: acme/hello` and `Version 0.1.0: Git commit and tag acme/hello@0.1.0.`

#### Scenario: init without --into
- **WHEN** the user runs `gitpm init acme/hello`
- **THEN** it fails with `init needs --into DIR: …` and creates nothing

#### Scenario: Former alias
- **WHEN** the user runs `gitpm create acme/hello --into hello`
- **THEN** it fails with `unknown command: create; use --help` and creates nothing
