## Decisions

Use version 1 for existing numeric format markers. Release records have no version marker and retain their existing shape. The OpenSpec configuration schema is a tooling selector, not a gitpm file format.

Legacy monolithic catalogs remain rejected by their `plugins` shape, independently of the numeric schema. Preserve that check and its error behavior. Unsupported numeric versions continue to fail without rewriting input.

Before the first public release, evolve formats in place without compatibility readers. Afterward, assess compatibility against the previous public release and bump affected formats in one separate step in the release milestone, at most once per release.
