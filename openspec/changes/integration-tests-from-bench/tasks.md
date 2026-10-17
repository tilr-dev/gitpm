# Tasks

## 1. Monorepo authoring and install

- [ ] 1.1 Create a synthetic multi-package repository like `make-repo.sh`: `init a/runtime --into REPO --multi --no-release`, then `init a/<name> --into REPO --no-release` for 4–5 packages, replace the demo dist, write metadata (`title`, `provides: "theme"` on one, `tags`), and release each with `commit REPO V --package ID`; assert one tag per package and no 0.1.0 demo tags
- [ ] 1.2 Install every package like `install.sh`; assert exactly one shared clone under `repos/` and `clone`/`path` per package in `list`

## 2. Repeat runs

- [ ] 2.1 Re-run the install loop: each line is `ID already installed (V)`, exit 0, workspace byte-identical (tree hash)
- [ ] 2.2 Release one package again and run `update` for all: one `updated A → B`, the rest `up to date (V)`

## 3. dist as the host's view

- [ ] 3.1 Assert `dist/` holds exactly the active packages after install, `disable`/`enable`, and that a host-style copy of `dist/*/*` needs no `list` call
- [ ] 3.2 `sync-dist --dry-run` on the finished workspace prints `dist/ already holds exactly the active plugins.`

## 4. Fork and switch back

- [ ] 4.1 Like `fork-demo.sh`: `fork a/theme --ns me --into forks/theme --replace`, edit, `commit forks/theme V --package me/theme`, `update me/theme`; assert `dist/me/theme` present and `dist/a/theme` absent
- [ ] 4.2 `replace a/theme --back`: both theme providers are deployed and the message says the fork stays installed and enabled; `uninstall me/theme --force` leaves only `dist/a/theme`

## 5. Metadata

- [ ] 5.1 `list --provides theme` returns the theme providers (including an inactive replaced one, with `active: false`) and the `null`/`[]` shape for absent keys
- [ ] 5.2 Run fmt, clippy and `cargo test --workspace`; note the new test file in README "Tests"
