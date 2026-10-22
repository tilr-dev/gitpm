## REMOVED Requirements

### Requirement: Library-owned Git input
**Reason**: Repository releases create one tag with `git tag -m MESSAGE`; no multi-tag `update-ref --stdin` transaction remains.
**Migration**: Release tags use the normal non-interactive Git subprocess path, honoring `tag.gpgSign`.
