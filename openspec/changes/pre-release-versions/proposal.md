# Proposal: npm-style versions and ranges, with pre-releases

## Why

Today versions are `MAJOR.MINOR.PATCH` only (`catalog::version_key` refuses anything else, spec `package-ids` "Version format"), dependency requirements follow Cargo (a bare `1.2.3` means `^1.2.3`, comma means AND, pre-release comparators are refused; spec `dependencies` "Requirement syntax"), and "pre-release versions in `update all`" was listed as not implemented when it shipped (Codex session 2026-10-16 20:52). The owner decided on 2026-10-18 (decision D9, TILR-41; TILR-16 merged into it): packages are mostly JavaScript, so gitpm follows npm semantics.

## What Changes

- **BREAKING** Requirement syntax as npm: `^` compatible, `~` patch-level, space-separated comparators are AND (`>=1.3.0 <2.0.0`), `*`; a bare version `1.2.3` means exactly 1.2.3; the comma separator goes away; no `||` and no hyphen ranges. 0.x keeps the npm/Cargo rule (`^0.2.3` = `<0.3.0`, `^0.0.3` = only 0.0.3). Migration: rewrite bare versions meant as ranges to `^V` (no compatibility before 1.0, TILR-38).
- `fork` and `fork --replace` keep writing rewritten dependency requirements as `^V` (multi-package-repos, unchanged); `init` writes `"dependencies": {}`. Bare versions now mean exact, so authors write `^` themselves.
- A top-level `update ID` / `update all` moves to the latest release even across a major version; such a line is marked `(major)` (`ID updated 1.2.3 → 2.0.0 (major)`, and `2.0.0 (major)` in `check`). Dependents stay protected by their own requirements (an update that breaks a dependent is still refused).
- Authors mark a breaking release only by its version number (major; minor for 0.x); no extra manifest field.
- Pre-releases `MAJOR.MINOR.PATCH-PRE` as npm: valid in manifests, tags and catalogs, ordered by SemVer, shown by the catalog and `check`, never picked implicitly by `install`/`update`/`update all`; installable explicitly (`--version 1.3.0-beta.1`, `--ref` of its tag); once installed, `update` follows pre-releases of the same `1.3.0` and the final `1.3.0`. A range like `^1.2.0` does not match pre-releases unless it names one of the same version (`^1.3.0-beta`).
- Build metadata (`+build`) stays refused.

## Capabilities

### Modified Capabilities
- `package-ids`: "Version format" (pre-releases, SemVer order, build metadata refused); "Release tag names" (`ID@1.3.0-beta.1` is a release tag).
- `dependencies`: "Requirement syntax" (npm ranges, bare version exact, pre-release matching).
- `updates`: new "Major updates are marked", "Pre-releases are opt-in"; "Stable update moves to the newest release", "Joint upgrade of all packages".
- `install-uninstall`: "Install the latest release of a Git source" (pre-releases not picked).
- `authoring`: "commit releases a version" (pre-release versions, build metadata refused).
- `catalog`: new "Catalog versions follow SemVer and show pre-releases" (ordering, latest excludes pre-releases, `check` marks `(major)`).
- `cli`: "One output line per affected package" (`(major)`).
- `ffi-bindings`: new "Major and pre-release flags in records" (adds `major`, `prerelease`).

`fork-replacement` and `multi-package-repos` need no delta: fork rewrites already write `^V`.

Depends on: `no-backward-compat`, `thin-cli-boundary`, `check-via-origin`, `single-ref-mode`, `download-and-enable`, `update-all-skips-broken`, `removal-and-restore-rules`, `no-tag-immutability`, `protected-catalog`.

## Out of scope

- Build metadata in versions.
- `||` alternatives and hyphen ranges.
- The `--ref` modes (`single-ref-mode`, TILR-40).
- Compatibility with manifests written for Cargo semantics (TILR-38).

## Impact

`crates/gitpm/src/deps.rs` (requirement parser, solver ranges), `catalog.rs` (`version_key`, ordering), release discovery, `upgrade.rs`/update selection, `check.rs`, scaffold/fork dependency writing, CLI output and `--version` validation, FFI records (`major`, `prerelease`), README "Changed in 0.6". Tracker: TILR-41.
