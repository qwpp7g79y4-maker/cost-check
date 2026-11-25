# Quality notes

Short, honest status of what is tested in this repository and how. Dutch version: [QUALITY.nl.md](QUALITY.nl.md).

## Tested commit

Date: 2026-11-24. Tested code commit: 28de6f3 (branch `chore/cargo-metadata`). Commits after it change documents only. It repairs findings B-1 and B-3 of the final review cost-eind-40eebd7 of tip 40eebd7. B-2 (E-N8 in the previous review) is still open and was not repaired. The earlier repairs (E-B1, E-B2 in 67f7133; B1 to B4 in 317aaf6) are unchanged. Findings N-a to N-l and N1 to N13 of the reviews are still open.

## Tests

`cargo test -j 2 --locked -- --test-threads=2` on Linux, debug profile, rustc 1.99.0, on 28de6f3:

- 60 unit tests in `src/lib.rs`, all green
- 0 tests in `src/main.rs`
- 44 integration tests in `tests/cli.rs`, all green
- 2 integration tests in `tests/golden.rs`, both green
- 1 test in `tests/live.rs`, ignored on purpose (see below)
- 106 passed, 0 failed, 1 ignored.

`cargo fmt --check` and `cargo clippy -j 2 --locked --all-targets -- -D warnings` are clean on Linux.

`tests/live.rs` runs the four real tools. It is ignored in a plain `cargo test`, because a machine without the tools proves nothing and a skipped test must not count as a pass. It was not run for this commit (not run).

## Red on the old tip (B-1, B-3)

The new tests were run against the source of 40eebd7 (the tests copied in, `src/sniff.rs` as it was) and are red there. This is the cargo output, shortened to the lines with a result:

```text
$ cargo test -j 2 --locked --no-fail-fast -- --test-threads=2      (source of 40eebd7, new tests)
test sniff::tests::jsonl_with_no_space_before_the_brace_is_a_log ... FAILED
thread 'sniff::tests::jsonl_with_no_space_before_the_brace_is_a_log' panicked at src/sniff.rs:202:9:
assertion `left == right` failed
  left: Dd
 right: Log
test result: FAILED. 59 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
test with_stubs::auto_reads_jsonl_that_starts_with_a_brace_as_a_log ... FAILED
thread 'with_stubs::auto_reads_jsonl_that_starts_with_a_brace_as_a_log' panicked at tests/cli.rs:1021:9:
stderr: cost-check: binary ontbreekt: dd-metric-bill (release-build of PATH)
test result: FAILED. 43 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.66s
```

The red tests, by finding:

- B-1: `jsonl_with_no_space_before_the_brace_is_a_log` (unit, `src/sniff.rs`: `{"level":"error","msg":"disk full","host":"a"}` with no space before the `{` must be `log-bill` with JSONL, and gave `dd-metric-bill`) and `auto_reads_jsonl_that_starts_with_a_brace_as_a_log` (`tests/cli.rs`, `--tool auto` must give a `log-bill` report). The second one was red because `auto` chose `dd-metric-bill`; the old source ends in an error instead of a report. The sniff test with a space before the `{` was already there and stays green.
- `a_prometheus_line_with_a_name_stays_prometheus` (`up{job="api"} 1`) is green on both: it guards the case that must not change.
- B-3: no test could be made red on Linux, because a CRLF checkout is not made here. It is covered by the file `.gitattributes`, not by a test. The CRLF checkout itself was not tried (not run).

On 28de6f3 all of them are green (see Tests).

## What the repair does

- B-1: `looks_like_prometheus` no longer treats an empty prefix before the first `{` as a metric name (`.all()` on an empty prefix is true). A line with a name before the `{` is still Prometheus. An explicit `--tool` was never affected. Nothing else in the detection changed.
- B-3: `.gitattributes` with `* text=auto eol=lf`, so the golden fixture `tests/fixtures/ci.md` has newlines only in every checkout. `Cargo.toml` has no `exclude`, so nothing had to be removed there. The built-in rates and the rounding rule did not change.

## Open

- B-2 (E-N8 in the previous review) is still open. It was not repaired. `pattern_month` was not changed and no `HashMap` was added for patterns.

## Earlier round (E-B1, E-B2, 67f7133)

The new tests were run against the source of 0e32634 (the tests copied in, the source untouched) and were red there. This is the cargo output, shortened to the lines with a result:

```text
$ cargo test -j 2 --no-fail-fast      (source of 0e32634, new tests)
test sniff::tests::a_cut_inside_a_multibyte_char_does_not_panic ... FAILED
test sniff::tests::a_state_file_with_a_cut_char_is_still_found ... FAILED
test serve::tests::an_upload_cut_inside_a_multibyte_char_is_a_report ... FAILED
thread 'sniff::tests::a_state_file_with_a_cut_char_is_still_found' panicked at src/sniff.rs:80:28:
end byte index 8192 is not a char boundary; it is inside '�' (bytes 8191..8194 of string)
test result: FAILED. 55 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test with_stubs::a_hostile_tool_field_is_one_escaped_line_on_stderr ... FAILED
test with_stubs::report_survives_a_char_cut_by_the_sniff_window ... FAILED
test with_stubs::the_stderr_of_a_failing_tool_is_one_escaped_line ... FAILED
test with_stubs::serve_survives_a_char_cut_by_the_sniff_window ... FAILED
test result: FAILED. 39 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.87s
```

The red tests, by finding:

- E-B1: `a_cut_inside_a_multibyte_char_does_not_panic` and `a_state_file_with_a_cut_char_is_still_found` (unit, `src/sniff.rs`: é, 日 and an emoji around byte 8192, with and without a BOM), `an_upload_cut_inside_a_multibyte_char_is_a_report` (`src/serve.rs`), `report_survives_a_char_cut_by_the_sniff_window` (auto and explicit `--tool`, exit not 101) and `serve_survives_a_char_cut_by_the_sniff_window` (the `serve` process must still run after the upload).
- E-B2: `a_hostile_tool_field_is_one_escaped_line_on_stderr` (a `tool` field with ESC, BEL and a newline) and `the_stderr_of_a_failing_tool_is_one_escaped_line` (a tool that writes ESC, BEL and a newline to stderr). Each must give one line, with no control character, that starts with `cost-check: `.

On 67f7133 all of them were green.

### What that repair did

- E-B1: `sniff_prefix` cut the lossy text at byte 8192. The lossy text can be longer than the bytes (a cut character becomes U+FFFD, three bytes), so the cut could fall inside it and panic (exit 101, and `serve` stopped). The cut now moves back to a character boundary. The detection itself did not change.
- E-B2: the message of a `Fault` goes through `escape_display` when `main` prints it, and the stderr of a failing tool is escaped (after the 500 character cut) before it enters the message. The report already did this. The built-in rates and the rounding rule did not change.

## Earlier round (B1 to B4, 317aaf6)

The new and changed tests were run against the source of 749cb16 (the tests copied in, the source untouched) and are red there. This is the cargo output, shortened to the lines with a result:

```text
$ cargo test -j 2 --no-fail-fast      (source of 749cb16, new tests)
test serve::tests::a_price_file_cannot_replace_the_export_of_the_same_name ... FAILED
test result: FAILED. 54 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test a_dd_tag_key_cannot_close_the_code_block ... FAILED
test a_dd_tag_key_cannot_add_a_second_line_after_allow_tags ... FAILED
test a_dd_tag_key_is_not_proposed_when_the_shell_would_run_it ... FAILED
test a_dd_total_shows_another_currency_apart ... FAILED
test a_dd_total_keeps_the_rounding_rule ... FAILED
test a_dd_total_counts_the_indexed_line_too ... FAILED
test a_log_pattern_cannot_add_a_second_line_to_the_drop_rule ... FAILED
test a_log_pattern_cannot_add_a_tab_to_the_drop_rule ... FAILED
test a_rum_type_name_cannot_close_the_code_block ... FAILED
test result: FAILED. 30 passed; 9 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
```

The red tests of that round, by finding:

- B1: `a_dd_tag_key_cannot_close_the_code_block`, `a_dd_tag_key_cannot_add_a_second_line_after_allow_tags`, `a_rum_type_name_cannot_close_the_code_block`, `a_log_pattern_cannot_add_a_second_line_to_the_drop_rule` and `a_log_pattern_cannot_add_a_tab_to_the_drop_rule`. The first and the third existed and demanded the raw newline in the block; they now demand one line per proposal.
- B2: `a_dd_tag_key_is_not_proposed_when_the_shell_would_run_it` (key `env$(id);x`).
- B3: `a_dd_total_counts_the_indexed_line_too` (ingested 900.00 plus indexed 1200.00 gave 900.00 and now gives 2100.00 USD), `a_dd_total_keeps_the_rounding_rule` and `a_dd_total_shows_another_currency_apart`.
- B4: `a_price_file_cannot_replace_the_export_of_the_same_name` (in `src/serve.rs`).

`a_dd_allowlist_of_plain_keys_is_still_proposed` is green on both: it guards the case that must not change. On 317aaf6 all of them were green.

### What that repair did

- B1: a field from the export that ends up in the proposed fix has its newline, carriage return, tab and other control characters written as visible escapes (`\n`, `\t`), as in a cell. The drop rule of `log-bill` and the type name of `rum-meter` use this.
- B2: `--allow-tags` is proposed only when every key is within `A-Za-z0-9_.:-` and at most 64 characters (the same check as the option). Otherwise the block says no allowlist is proposed. No key is quoted, so none can reach a shell.
- B3: `dd-metric-bill` adds all cost lines of one currency before it rounds. A line in another currency is shown apart. The rounding rule did not change, and no built-in rate changed.
- B4: `serve` writes the export and the price file with `unique_path`, so a price file with the name of the export gets another name.

## Platforms

Tests that run a stub use `sh` and a file mode (`set_mode`), and the test of the mode of the work folder reads Unix permissions. Those tests are compiled only on Unix (`#[cfg(unix)]`); on Windows they are left out. The tests that need Linux (`/dev/full`, a byte that is not UTF-8 in a file name) are compiled only on Linux.

## Not run

- The GitHub Actions run of `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): not run (niet uitgevoerd). Nothing was pushed.
- `cargo test --release`, `cargo +1.85 test`, the Windows and macOS targets with `cargo check` and `cargo clippy`, `cargo deny` and `cargo audit`: not run for 28de6f3. Earlier results for 7a5a3d6 do not carry over.
- `tests/live.rs` with the real tools: not run.
- A CRLF checkout (`core.autocrlf=true`) of the golden fixture: not run.
- No test counts the headings of the README files.
- Open findings B-2, N-a to N-l and N1 to N13: not repaired and not touched.

## CI

`.github/workflows/ci.yml` has three jobs, on every push and pull request. `check` runs `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` on `ubuntu-latest`, `windows-latest`, `macos-latest` and `macos-15-intel`. `msrv` runs `cargo check --locked --all-targets` and `cargo test --locked` on Rust 1.85. `deny` runs `cargo deny --locked check`. None of these has run on GitHub yet (see Not run).
