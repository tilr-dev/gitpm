## Implementation
- [x] Describe the tiers, the events and the nightly run in release-process.
- [x] Add the nextest profiles (full and fast) with the boundary chosen by measurement.
- [x] Switch `test` to nextest by event, drop the push to `main`, add the nightly and manual runs, stop saving caches in the merge queue.
- [x] Add a contract test for the workflow and run it in CI.
- [x] Verify the hermetic Git environment under nextest and that both tiers pass locally.
