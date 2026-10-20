# Design

## Decision

Scan the root directory entries before initialization and permit the exact name
`.DS_Store`. Propagate directory read errors and keep the existing `workspace`
error for every other name. Do not delete or rewrite Finder metadata.

Deleting `.DS_Store` before the emptiness check would lose user bytes; allowing
all hidden files would weaken the foreign-directory safeguard.

## Transactions and recovery

This changes only the admission check. The existing lock, state initialization,
transaction recovery and crash behavior remain in place. No format migration or
new transaction is introduced.

## Validation

The positive unit test fails before the fix and checks initialized layout and
preserved metadata. Mixed ordinary and hidden files remain refused without
changing the root. The Dagger `.DS_Store root` scenario exercises CLI creation
through `sync-dist`.
