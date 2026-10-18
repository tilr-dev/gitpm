## MODIFIED Requirements

### Requirement: Requirement syntax
**BREAKING** `dependencies` in the manifest SHALL map package IDs to npm-style ranges: a bare full version `1.2.3` (or `=1.2.3`) means exactly that version; `^1.2.3` compatible (`>=1.2.3 <2.0.0`; 0.x: `^0.2.3` = `>=0.2.3 <0.3.0`, `^0.0.3` = only 0.0.3); `~1.2.3` patch-level (`>=1.2.3 <1.3.0`); the comparators `>`, `>=`, `<`, `<=` separated by spaces are AND (`>=1.3.0 <2.0.0`); `*` any release. `^`, `~` and the comparators also take a partial version as npm does (`^1.2` = `>=1.2.0 <2.0.0`, `<2` = `<2.0.0`). A pre-release version satisfies a range only when the range contains it and one of its comparators names a pre-release of the same MAJOR.MINOR.PATCH. A bare partial version (`1.2`), wildcards other than `*` (`1.*`, `1.x`), `||`, hyphen ranges, commas and unknown operators SHALL be refused with `invalid_manifest`. Every key MUST be a valid package ID (`invalid_id` otherwise) and a package MUST NOT depend on itself (`invalid_manifest`, "package ID depends on itself"). Verified by `deps::requirements_follow_npm_semantics`.

#### Scenario: Caret on 0.x
- **WHEN** a dependency requires `^0.2`
- **THEN** 0.2.5 satisfies it and 0.3.0 does not

#### Scenario: Caret on 0.0.x
- **WHEN** a dependency requires `^0.0.3`
- **THEN** only 0.0.3 satisfies it

#### Scenario: Bare version is exact
- **WHEN** a dependency requires `1.2.3`
- **THEN** only 1.2.3 satisfies it; `1.2.4` does not

#### Scenario: Self-dependency
- **WHEN** `acme/app`'s manifest lists `acme/app` in `dependencies`
- **THEN** the manifest is refused with `invalid_manifest`

#### Scenario: Pre-release requirement
- **WHEN** a dependency requires `^1.3.0-beta.1`
- **THEN** `1.3.0-beta.2`, `1.3.0` and `1.4.0` satisfy it, while `1.4.0-beta.1` does not

#### Scenario: Range does not match pre-releases
- **WHEN** a dependency requires `^1.2.0` and `1.3.0-beta.1` is the newest version
- **THEN** `1.3.0-beta.1` does not satisfy it

#### Scenario: Old Cargo syntax
- **WHEN** a dependency requires `>=1.3, <2`, `1.*`, `1.2.0 - 1.4.0` or `^1 || ^2`
- **THEN** the manifest is refused with `invalid_manifest`; the author writes `>=1.3.0 <2.0.0` or `^1`
