## MODIFIED Requirements

### Requirement: Requirement syntax
`dependencies` SHALL map package IDs to npm-style ranges: a bare full version `1.2.3` (or `=1.2.3`) is exact; `^`, `~`, the comparators `>`, `>=`, `<`, `<=` (space-separated: AND) and `*` work as in npm. A pre-release satisfies a range only when a comparator names a pre-release of its MAJOR.MINOR.PATCH. Other syntax SHALL be refused with `invalid_manifest`. Keys MUST be valid IDs (`invalid_id`) other than the own ID (`invalid_manifest`). Verified by `deps::requirements_follow_npm_semantics`.

#### Scenario: Range forms
- **WHEN** dependencies require `^1.2.3`, `^1.2`, `~1.2.3`, `>=1.3.0 <2.0.0` and `<2`
- **THEN** they mean `>=1.2.3 <2.0.0`, `>=1.2.0 <2.0.0`, `>=1.2.3 <1.3.0`, both comparators at once, and `<2.0.0`

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
- **WHEN** `b879842f-16c2-558f-9d03-a271dc901f57`'s manifest lists `b879842f-16c2-558f-9d03-a271dc901f57` in `dependencies`
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

### Requirement: Dependents guard removal
Single source for `has_dependents`: `disable`, `forget` and `uninstall` SHALL refuse with it unless forced exactly when a consumer's dependency would break: one resolving to the package directly, or through a binding ORIGINAL does not take over (installed, enabled, matching version). `dependents` and `UninstallPlan.dependents` SHALL name exactly those consumers. Verified by `dependents_guard_disable_forget_and_uninstall` and `removing_an_active_fork_guards_only_consumers_the_original_breaks`.

#### Scenario: Uninstall a replacing package with consumers
- **WHEN** `app` depends on `d135a7cf-1c63-549a-93cb-8f89656b2ced`, which `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces, `d135a7cf-1c63-549a-93cb-8f89656b2ced` is not installed, and the user runs `uninstall 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** it fails with `has_dependents` naming `app`

#### Scenario: Forget a replacing package without consumers
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces `d135a7cf-1c63-549a-93cb-8f89656b2ced`, nothing depends on `d135a7cf-1c63-549a-93cb-8f89656b2ced`, and the user runs `forget 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** it succeeds without force and drops the binding (fork-replacement, "Active fork rules")

#### Scenario: Disable an active replacement without force
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces `d135a7cf-1c63-549a-93cb-8f89656b2ced`, `app` needs `d135a7cf-1c63-549a-93cb-8f89656b2ced ^2`, the installed `d135a7cf-1c63-549a-93cb-8f89656b2ced` is 1.4.0, and the user runs `disable 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`
- **THEN** the command fails with `has_dependents` naming only `app` and both deployed folders are unchanged
- **AND** without such a consumer the disable succeeds, drops the binding and deploys `d135a7cf-1c63-549a-93cb-8f89656b2ced` again; this is covered by `disable_removes_and_enable_restores_also_around_a_replacement`

#### Scenario: Installed original satisfies the consumers
- **WHEN** `8f0b16d1-526c-56fd-bdae-b6b991bbcb76` replaces `d135a7cf-1c63-549a-93cb-8f89656b2ced`, `app` needs `d135a7cf-1c63-549a-93cb-8f89656b2ced ^1` and the installed, enabled `d135a7cf-1c63-549a-93cb-8f89656b2ced` is 1.4.0
- **THEN** `uninstall 8f0b16d1-526c-56fd-bdae-b6b991bbcb76`, `forget 8f0b16d1-526c-56fd-bdae-b6b991bbcb76` and `disable 8f0b16d1-526c-56fd-bdae-b6b991bbcb76` succeed without force and `app` resolves to `d135a7cf-1c63-549a-93cb-8f89656b2ced` again

#### Scenario: Uninstall a dependency
- **WHEN** `app` depends on `lib` and the user runs `gitpm uninstall lib`
- **THEN** it fails with `has_dependents` naming `app`; with `--force` lib is removed

#### Scenario: Forced removal and switching back
- **WHEN** the user passes --force to disable, forget or uninstall
- **THEN** removal is allowed and broken consumers are reported; returning through replace ORIGINAL --with ORIGINAL has no force bypass

### Requirement: Broken consumers are reported, not enforced
After a forced removal, already broken consumers SHALL be reported (in `list` dependency entries, `check` "Unsatisfied dependency" lines and as skipped in `update all`) but SHALL NOT block unrelated operations; `update all` SHALL leave them at their installed revision, out of its solve, and SHALL never install a dependency that was removed with force for them. Verified by `update_all_skips_broken_consumers_and_never_reinstalls`.

#### Scenario: Unrelated install after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, and the user installs `other`
- **THEN** the install succeeds and `list` reports `app`'s dependency on `lib` as unsatisfied

#### Scenario: update all after forced removal
- **WHEN** `lib` was force-uninstalled while `app` depends on it, a catalog that has `lib` is configured (or none is), and the user runs `update all`
- **THEN** the other packages are updated, `app` is in `unchanged` with reason `broken_dependency` (`dependency cfbc4d17-8707-5571-853d-c9136a7f9f21 is not installed`), `lib` is not installed again, and `list` and `check` still report `app`'s dependency unsatisfied

### Requirement: enable needs active dependencies
`enable ID` SHALL refuse with `dependency_conflict` naming every direct dependency no active package satisfies (missing, disabled, replaced away, other version) and SHALL NOT enable dependencies itself; `enable ID --force` SHALL enable it anyway (the consumer is reported broken, "Broken consumers are reported, not enforced"). An already enabled package is not checked. Verified by `enable_refuses_inactive_dependencies_unless_forced` and `dropping_a_binding_checks_the_original_s_dependencies`.

#### Scenario: Disabled dependency
- **WHEN** `app` requires `lib ^1`, both are installed and disabled, and the user runs `enable app`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes

#### Scenario: Forced enable
- **WHEN** the user runs `enable app --force` in the same workspace
- **THEN** `app` is enabled and deployed, and `list` reports its dependency on `lib` as unsatisfied

#### Scenario: Original with an inactive dependency
- **WHEN** `77f9f0bc-8e1b-5376-992a-c8922c08e31e` replaces `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9`, `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` requires `lib ^1`, `lib` is disabled, and the user runs `disable 77f9f0bc-8e1b-5376-992a-c8922c08e31e` or `replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9`
- **THEN** it fails with `dependency_conflict` ("lib is disabled; enable it") and nothing changes; `disable 77f9f0bc-8e1b-5376-992a-c8922c08e31e --force` disables it anyway and `c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9` is reported broken

#### Scenario: Reactivated original
- **WHEN** dropping a binding makes ORIGINAL active again (`disable`, `forget` or `uninstall` of the replacing package, or `replace ORIGINAL --with ORIGINAL`)
- **THEN** ORIGINAL's direct dependencies are checked the same way: the removals refuse with `dependency_conflict` unless forced, `replace ORIGINAL --with ORIGINAL` refuses without a force bypass
