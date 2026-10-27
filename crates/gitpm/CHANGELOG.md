# Changelog

## [0.6.0](https://github.com/tilr-dev/gitpm/compare/v0.5.0...v0.6.0) (2026-10-27)


### ⚠ BREAKING CHANGES

* remove fork --replace; fork no longer releases, installs or replaces packages
* fork packages as a Git clone of their recorded source
* reset gitpm file format versions to 1 until the first public release
* refuse deploying a package with a submodule inside dist
* one SSH policy and Options.custom_ssh for auth check and all Git commands
* commit refuses a Git identity that Git derived on its own
* npm-style version ranges, major marks and opt-in pre-releases
* verified catalogs protect the packages installed from them
* moved release tags never block updates; rollback sees origin tags
* one removal rule for active forks, planned uninstall, removal without a clone
* update all skips broken consumers and names every kept-back reason
* download without enabling; a disabled package satisfies no dependency
* one --ref parameter with a recorded mode instead of --channel
* check reads each package's origin when it is not in a catalog
* move all command logic into the library, thin CLI
* drop backward compatibility and rename plugin to package

### Features

* check reads each package's origin when it is not in a catalog ([7ea6307](https://github.com/tilr-dev/gitpm/commit/7ea63073477a6b00edccd3d99bd08bfea9fab5e9))
* download without enabling; a disabled package satisfies no dependency ([5dc8df8](https://github.com/tilr-dev/gitpm/commit/5dc8df8d1de2346f4ab5765c5405a745b1351524))
* drop backward compatibility and rename plugin to package ([f3ee106](https://github.com/tilr-dev/gitpm/commit/f3ee10655b617913ccec0b96a4847278068b1f91))
* fork packages as a Git clone of their recorded source ([fae3c96](https://github.com/tilr-dev/gitpm/commit/fae3c965ee7eec645cc13ac4167d577effa8bc8b))
* let the package manifest choose its install folder and describe its build ([01b245b](https://github.com/tilr-dev/gitpm/commit/01b245b02e866077e0c2125415c0017f356aac5a))
* moved release tags never block updates; rollback sees origin tags ([661dd01](https://github.com/tilr-dev/gitpm/commit/661dd01765128be13c9d51ec3529f7cce5fdf893))
* npm-style version ranges, major marks and opt-in pre-releases ([0443fda](https://github.com/tilr-dev/gitpm/commit/0443fda8af310fc1259d040c01ac578a1a012247))
* one --ref parameter with a recorded mode instead of --channel ([5a9ddbf](https://github.com/tilr-dev/gitpm/commit/5a9ddbf66f8b7fef93f35c209796df462244e277))
* one removal rule for active forks, planned uninstall, removal without a clone ([df21d4a](https://github.com/tilr-dev/gitpm/commit/df21d4ab2f0262a9e6fe1ca86a8d262564957b14))
* publish the latest release summary in catalog package indexes ([ec10a9e](https://github.com/tilr-dev/gitpm/commit/ec10a9e8ff8ae8e41de31fb6ed8524db147b4919))
* remove fork --replace; fork no longer releases, installs or replaces packages ([27ceb64](https://github.com/tilr-dev/gitpm/commit/27ceb64288cd4292fddc4a4ef250ab0f1f7d4740))
* update all skips broken consumers and names every kept-back reason ([704289e](https://github.com/tilr-dev/gitpm/commit/704289e48072aa4370cc838faeb32f01ef0dec2e))
* verified catalogs protect the packages installed from them ([ae7afda](https://github.com/tilr-dev/gitpm/commit/ae7afdae7c502c5bc81ec5f49dcae28b13e9dba2))


### Bug Fixes

* a refused second lock attempt no longer releases the first holder's lock ([a6bfe13](https://github.com/tilr-dev/gitpm/commit/a6bfe13a6db48876aed1cdc327fb573932262670))
* bound auth check and the core.sshCommand probe to 10 seconds ([df10335](https://github.com/tilr-dev/gitpm/commit/df103353a51b445edd3f0512594deed25ac755bd))
* clone local sources over Git transport instead of copying objects/ ([c8cb52f](https://github.com/tilr-dev/gitpm/commit/c8cb52fb437771186ceac9a29008f8335b9747d9))
* commit refuses a Git identity that Git derived on its own ([c2224d6](https://github.com/tilr-dev/gitpm/commit/c2224d64e4683e7af88e7f15c8477314c3c4a022))
* deploy and hash the committed dist without Git attributes ([36b1a87](https://github.com/tilr-dev/gitpm/commit/36b1a87e23e0eb559cd0a7db89a5e7d0f4a7f2fd))
* fsync the journal, state.json and directories on transaction commit ([d6dc2bd](https://github.com/tilr-dev/gitpm/commit/d6dc2bd302b27d645910e0598b857e5ff906eae3))
* git never waits for input through askpass or a credential helper ([681b900](https://github.com/tilr-dev/gitpm/commit/681b9003e204cb1895473a726ac07a03cd5b566b))
* name the package in update, rollback and update all refusals ([a9372c3](https://github.com/tilr-dev/gitpm/commit/a9372c39aa1c4b181b23b65b8ee1c60bedabfa6d))
* one SSH policy and Options.custom_ssh for auth check and all Git commands ([145f268](https://github.com/tilr-dev/gitpm/commit/145f2680a25bab1d6d95e2c59ff67380ae9607f0))
* refuse a streamed catalog response larger than 16 MiB ([db1a0bb](https://github.com/tilr-dev/gitpm/commit/db1a0bb0611ca0ff6cdbc270044ce352891a264d))
* refuse deploying a package with a submodule inside dist ([6a0da14](https://github.com/tilr-dev/gitpm/commit/6a0da140d7a2a05b4f08af652f5d3fe0006ac9c3))
* release fork --replace through the commit path ([b5504c3](https://github.com/tilr-dev/gitpm/commit/b5504c30bf180c29c26d5e61384f880bda68b9a7))
* reset gitpm file format versions to 1 until the first public release ([d140e0d](https://github.com/tilr-dev/gitpm/commit/d140e0d7416d915cb69898fcf4eb44184c89c089))
* sync-dist reports active packages blocked by a non-folder dist/&lt;ns&gt; ([d5da127](https://github.com/tilr-dev/gitpm/commit/d5da1275e593cabcc0ea59c4efc97a95d92b6451))
* treat a folder holding only .DS_Store as empty when creating a workspace ([2032490](https://github.com/tilr-dev/gitpm/commit/20324909ea88896aed33120d1aaff38eb9575f33))
* write a new workspace's state.json atomically via a temp file and rename ([a0056b6](https://github.com/tilr-dev/gitpm/commit/a0056b683ed498b5d65c494b09c56c731c028417))


### Code Refactoring

* move all command logic into the library, thin CLI ([b27832b](https://github.com/tilr-dev/gitpm/commit/b27832b214aeb622e9bec92d70e1a55372e14f16))

## [0.5.0](https://github.com/tilr-dev/gitpm/compare/v0.4.0...v0.5.0) (2026-10-17)


### Features

* dist holds only active packages ([#13](https://github.com/tilr-dev/gitpm/issues/13)) ([3819836](https://github.com/tilr-dev/gitpm/commit/381983683796bd1b43948fcbe6e7f23373786d03))
* plugin metadata and CLI polish ([#11](https://github.com/tilr-dev/gitpm/issues/11)) ([6c2abf1](https://github.com/tilr-dev/gitpm/commit/6c2abf1b264bdfd229ed17c999715252bae4b0ff))
* unify command output, idempotent install, per-command help ([#14](https://github.com/tilr-dev/gitpm/issues/14)) ([6ea8030](https://github.com/tilr-dev/gitpm/commit/6ea8030abb5ec48bbc3d54a9382e1284c392cacc))

## [0.4.0](https://github.com/tilr-dev/gitpm/compare/v0.3.0...v0.4.0) (2026-10-17)


### ⚠ BREAKING CHANGES

* the fork ID NAMESPACE/NAME form and init --in are removed; --into DIR is required and names the repository itself. Library: Manager::fork with a default namespace, identity namespace config and scaffold::create/create_at/create_namespaced are removed. 0.4 is not downgrade-safe: 0.3 ignores clone/path/forgotten in state.json and cannot install catalog releases with path.

### Features

* multi-package repositories ([#9](https://github.com/tilr-dev/gitpm/issues/9)) ([744809d](https://github.com/tilr-dev/gitpm/commit/744809ddfb7657ec6b8ace7d63ba6f10560248fb))

## [0.3.0](https://github.com/tilr-dev/gitpm/compare/v0.2.0...v0.3.0) (2026-10-16)


### Features

* resolve and apply compatible updates for all plugins ([#5](https://github.com/tilr-dev/gitpm/issues/5)) ([1680bfc](https://github.com/tilr-dev/gitpm/commit/1680bfc6f8b55cb6cb7994a540b2e193ca025340))

## [0.2.0](https://github.com/tilr-dev/gitpm/compare/v0.1.1...v0.2.0) (2026-10-16)


### Features

* add gitpm-cli crate with the gitpm binary ([#3](https://github.com/tilr-dev/gitpm/issues/3)) ([6ffb2ea](https://github.com/tilr-dev/gitpm/commit/6ffb2ea3f78f8f803aef98cce321a91cf21fadec))

## [0.1.1](https://github.com/tilr-dev/gitpm/compare/v0.1.0...v0.1.1) (2026-10-16)


### Bug Fixes

* **deps:** bump sha2 to 0.11 ([#1](https://github.com/tilr-dev/gitpm/issues/1)) ([d7dd2d8](https://github.com/tilr-dev/gitpm/commit/d7dd2d8f1b828364797185949ff9ec58017a6221))
