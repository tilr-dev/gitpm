# Changelog

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
