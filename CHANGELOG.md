# Changelog

## [0.1.0] - 2026-11-25

Changes made after 2026-11-25; the version number did not change.

### Fixed

- Windows compiles again: the stubs and the permission test are Unix only (`cfg(unix)`). CI also runs `cargo deny` and Rust 1.85, and every cargo step uses `--locked`.
- An explicit `--tool` is no longer overruled by the guess from the first 8 KiB of the export.
- `--out` writes a new file exclusively and then renames it, and refuses a symlink. `--out` equal to `--input` exits 2 and leaves the export alone.
- Text from an export is made safe in the report for every tool: control, format and bidi characters become visible escapes, Markdown that starts a link, HTML or code gets a backslash, and a code block uses a fence longer than any run of backticks in it.
- A full standard output makes `serve` and `render` exit 2 instead of panicking. On Linux a closed standard output exits 2 too.
- A multipart field name no longer depends on the order of the parameters.
- No path of the development machine in the source. A tool is looked for next to the program and on `PATH`, also with `.exe`. The live test is ignored instead of silently skipped.
- A repeated key in one JSON object exits 2. A tag that is unbounded in another metric names that metric and counts only its series.
- A path with a byte that is not UTF-8 reaches the tool unchanged, and a path that starts with `-` is read as a file.

- `dd-metric-bill`: `Total net` adds every cost line of one currency (ingested and indexed) and shows another currency apart. The rounding did not change.
- A field from an export no longer adds a line to the suggested fix: newline and tab become visible escapes in the drop rule of `log-bill` and in the type name of `rum-meter`.
- `--allow-tags` is proposed only when every key is within `A-Za-z0-9_.:-` (at most 64 characters), so a key cannot reach a shell. Otherwise the block says no allowlist is proposed.
- `serve` gives the export and the price file different names when they would be the same.

- `--tool auto` no longer panics when the first 8192 bytes of the export end inside a multibyte character (é, 日, an emoji). `report` exited 101 and `serve` stopped; now the cut moves back to a character boundary.
- An error on stderr is shown safely: the message of a failure and the stderr of a failing tool have control characters written as visible escapes, so one error stays one line.

- `--tool auto` no longer reads a JSONL line that starts directly with `{` (`{"level":"error",...}`) as Prometheus text. An empty prefix before the `{` is no metric name; before this, the export went to `dd-metric-bill` and exited 2.
- `.gitattributes` (`* text=auto eol=lf`), so a CRLF checkout does not turn the golden fixture `tests/fixtures/ci.md` red.
- A log-bill export with many patterns and a price file, and a dd-metric-bill export with many tag keys, no longer take time that grows with the square of the input (B-2). 80 000 priced patterns took 39 s and now take 0.3 s; the report is the same byte for byte.

### Tests

- New tests for the cut multibyte character (unit, `serve` and the `cost-check` binary) and for the escaped errors on stderr. They are red on 0e32634 and green on 67f7133.
- New tests for JSONL with no space before the `{` (unit and `cost-check` binary). They are red on 40eebd7 and green on 28de6f3.
- The half cent is fixed in tests (284.205 gives 284.21, -0.005 gives -0.01, 1.005 gives 1.01). The rounding did not change.

### Added

- Documentation: `README.nl.md`, `CONTRIBUTING.md`, `SECURITY.md`, `QUALITY.md`, `QUALITY.nl.md` and this changelog.
- `deny.toml`, so `cargo deny check licenses` accepts only permissive licenses.

### Changed

- `README.md` is in English and has separate Installation, Quick start, Options, Usage, Example, Exit codes, Limitations, License and Maintenance sections.
- The test job in CI runs on Windows and macOS as well as Linux.
- `Cargo.toml` has `repository`, `homepage` and `documentation`.


First version: `cost-check report`, `render` and `serve`, with one report shape for `ci-bill`, `log-bill`, `dd-metric-bill` and `rum-meter`.
