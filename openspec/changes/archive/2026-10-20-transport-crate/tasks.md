## 1. Specification
- [x] Describe the transport module boundary and typed network methods.

## 2. Implementation
- [x] Move subprocess code and tests to the internal transport module, named by meaning.
- [x] Route library network operations through typed Transport methods and preserve errors.
- [x] Exercise typed clone/fetch/lookup and HTTP bounds with deterministic tests.

## 3. Validation
- [x] Self-review, full checks including Dagger, and finish.

## 4. Review return (Dan #4414)
- [x] Fold the crate into gitpm, preserve the trait and tests, enforce the module boundary.
- [x] Type remote HEAD refresh and cover it with the network fixture.
- [x] Correct the network scenario; rerun self-review and checks.
