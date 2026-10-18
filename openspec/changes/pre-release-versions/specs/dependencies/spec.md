## MODIFIED Requirements

### Requirement: Requirement syntax
`dependencies` in the manifest SHALL map package IDs to npm-style ranges: a bare version `1.2.3` means exactly that version, `^1.2.3` compatible (0.x: `^0.2.3` = `>=0.2.3 <0.3.0`), `~1.2.3` patch-level, space-separated comparators are AND (`>=1.3.0 <2.0.0`), `*` any. `||`, hyphen ranges, commas and unknown operators SHALL be refused (`invalid_manifest`). Keys MUST be valid IDs (`invalid_id`) other than the own ID (`invalid_manifest`). Verified by `deps::requirements_follow_npm_semantics`.

#### Scenario: Caret on 0.x
- **WHEN** a dependency requires `^0.2`
- **THEN** 0.2.5 satisfies it and 0.3.0 does not

#### Scenario: Self-dependency
- **WHEN** `acme/app`'s manifest lists `acme/app` in `dependencies`
- **THEN** the manifest is refused with `invalid_manifest` ("plugin ID depends on itself")

#### Scenario: Pre-release requirement
- **WHEN** a dependency requires `^1.3.0-beta.1`
- **THEN** `1.3.0-beta.2` and `1.3.0` satisfy it, while `1.4.0-beta.1` does not (a pre-release matches only a range that names a pre-release of the same MAJOR.MINOR.PATCH)

#### Scenario: Caret on 0.0.x
- **WHEN** a dependency requires `^0.0.3`
- **THEN** only 0.0.3 satisfies it

#### Scenario: Bare version is exact
- **WHEN** a dependency requires `1.2.3`
- **THEN** only 1.2.3 satisfies it; `1.2.4` does not

#### Scenario: Range does not match pre-releases
- **WHEN** a dependency requires `^1.2.0` and `1.3.0-beta.1` is the newest version
- **THEN** `1.3.0-beta.1` does not satisfy it

#### Scenario: Old Cargo syntax
- **WHEN** a dependency requires `>=1.3, <2`
- **THEN** the manifest is refused with `invalid_manifest`; the author writes `>=1.3.0 <2.0.0`
