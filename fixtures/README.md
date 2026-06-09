# fixtures/

Test inputs for `udf-core`. **No `.udf` files are committed to the repo** — they may carry
confidential legal content, so the whole `*.udf` pattern is gitignored.

- **Local sample** (untracked): drop a `sample.udf` here to enable the richer
  `parses_local_sample_with_image` assertions. The test skips cleanly when it is absent (e.g.
  in CI), so the committed suite never depends on it.
- **Real samples** (never committed): point the `UDF_SAMPLE_DIR` env var at a local folder of
  `.udf` files; the integration sweep asserts each parses and produces well-formed HTML +
  brace-balanced RTF.

The deterministic core coverage lives in synthetic, in-memory `content.xml` fixtures inside
`crates/udf-core/tests/integration.rs`, which need no external files.
