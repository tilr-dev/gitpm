## MODIFIED Requirements

### Requirement: Foreign directories are refused
Opening a non-empty directory (one holding anything but `.DS_Store`) that has no `state.json` SHALL fail with `workspace` ("refusing nonempty directory without state.json") and change nothing inside that directory (its parent folder and the workspace lock file beside it may already exist). Single source for a corrupt `state.json`: it SHALL block every operation with `invalid_data` and MUST NOT be overwritten.

#### Scenario: Pointing the workspace at a project folder
Verified by `workspace::open_refuses_foreign_directories_and_bad_state`.
- **WHEN** the workspace root is an existing non-empty folder without `state.json`
- **THEN** opening fails with `workspace` and no file is created in it (its parent and the lock file beside it may already exist)

#### Scenario: Corrupt state
Verified by `robustness::corrupt_state_is_not_overwritten`.
- **WHEN** `state.json` is not valid JSON
- **THEN** every operation fails with `invalid_data` and the file keeps its bytes

#### Scenario: Folder with only .DS_Store
Verified by `workspace::open_ds_store_only_root`.
- **WHEN** the workspace root exists and holds only `.DS_Store`
- **THEN** opening treats it as empty and creates the workspace in it
- **AND** `.DS_Store` keeps its bytes

#### Scenario: Folder with .DS_Store and other content
Verified by `workspace::open_ds_store_with_other_content_is_refused`.
- **WHEN** the workspace root holds `.DS_Store` and another file without `state.json`
- **THEN** opening fails with `workspace` and changes nothing inside it
