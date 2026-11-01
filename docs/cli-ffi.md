# CLI to native API mapping

Names below use the generated Swift spelling. Free functions take explicit locations; manager methods use the opened workspace. CLI flags select the corresponding typed parameters and options. Group entries `catalog` and `dependency` provide CLI help; their native APIs are the catalog reads and dependency edits.

| CLI command | FFI function |
| --- | --- |
| `init` | `init` |
| `fork` | `forkSource / PackageManager.forkPackages` |
| `packages` | `repositoryPackages` |
| `dependency / dependency add / dependency update / dependency remove` | `editDependency / PackageManager.editDependency` |
| `remote` | `setRemote` |
| `auth (auth check)` | `authCheck` |
| `catalog / catalog show` | `browseCatalog` |
| `catalog links` | `catalogLinks` |
| `catalog update` | `catalogUpdate` |
| `catalog register` | `catalogRegister` |
| `commit` | `commit` |
| `install-catalog` | `PackageManager.installFromCatalog` |
| `install` | `PackageManager.install` |
| `link` | `PackageManager.link` |
| `download-catalog` | `PackageManager.downloadFromCatalog` |
| `download` | `PackageManager.download` |
| `list` | `list / readState` |
| `check` | `check` |
| `update` | `PackageManager.update / updateAll / updateAllPlan` |
| `rollback` | `PackageManager.rollback` |
| `enable` | `PackageManager.enable` |
| `disable` | `PackageManager.disable` |
| `forget` | `PackageManager.forget` |
| `restore` | `PackageManager.restore` |
| `uninstall` | `PackageManager.uninstallPlan / PackageManager.uninstall / uninstallRefusal` |
| `replace` | `PackageManager.replacePlan / replace` |
| `replace-check` | `PackageManager.replaceCheck` |
| `replace-resolve` | `PackageManager.replaceResolve` |
| `sync-dist` | `PackageManager.syncDistPlan / syncDist` |

`ffi_covers_every_cli_command` checks the CLI table and UniFFI export annotations.
Swift bindings and the universal xcframework are generated together by
`scripts/build-apple.sh --release` and attached to release artifacts by the existing
release workflow. Generated bindings are build outputs, not checked-in sources.
