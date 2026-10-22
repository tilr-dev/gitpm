## MODIFIED Requirements

### Requirement: Git identity is never invented in existing repositories
gitpm SHALL NOT write a Git identity into an existing repository; without a configured identity there (`user.useConfigOnly`), `commit` and `init` into an existing repository SHALL be refused with `invalid_argument`. Only a repository gitpm creates itself (new `init` folder, new fork) MAY get the local identity `Package Demo <demo@example.invalid>` when the user has none. Verified by `scaffold::create_package_refuses_uncommitted_config_and_a_missing_identity`.

#### Scenario: Tag-only retry without identity
- **WHEN** an existing repository has no identity and `commit` is retried after a failed tag
- **THEN** the identity is not checked and the lightweight tag is created

#### Scenario: No identity
- **WHEN** git cannot determine a user identity for an existing repository and the author runs `gitpm commit REPO 0.2.0`
- **THEN** the command fails with `invalid_argument` ("no Git identity configured; set user.name and user.email") and the repository is unchanged
- **AND** an identity git would derive on its own from the login and host name is not accepted; `user.name`/`user.email` in any Git config, or the `GIT_AUTHOR_*`/`GIT_COMMITTER_*` variables, count; this is covered by `scaffold::derived_identity_is_refused`
