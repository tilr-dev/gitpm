## MODIFIED Requirements

### Requirement: Untracked files are detected regardless of git config
Local-work detection SHALL see untracked and ignored files whatever the repository's `status.showUntrackedFiles` setting (every status listing passes `--untracked-files=all`), and SHALL bound the listing of unpublished commits. Verified by `lost_work::changed_paths_sees_untracked_files_whatever_the_status_config_says` and `lost_work::unpublished_commit_listing_is_bounded`.

#### Scenario: Hidden untracked file
- **WHEN** the clone sets `status.showUntrackedFiles=no` and has an untracked file
- **THEN** local-work detection still reports it as an uncommitted change
- **AND** uninstall planning uses the same status listing
