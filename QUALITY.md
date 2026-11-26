# Quality notes

pep: 10

B-2 is repaired in 6778ec7 (see below), so no open B or B- remains and the tool is done. No open B, B-, M, M-, S or S- is left: 10 − 0 = 10. Other open points in this repository have no class and do not count.

Short, honest status of what is tested in this repository and how. Dutch version: [QUALITY.nl.md](QUALITY.nl.md).

## Tested commit

Date: 2026-11-26. Tested code commit: 6778ec7. Commits after it change documents only.

6778ec7 repairs B-2. The log report looked up the price of every pattern with a scan of all of `costs.patterns`, and the dd report did the same for every tag key (`collect_tags`) and for the allowlist (`suggest_allow`), so the time grew with the square of the input. Each lookup now goes through one index that is built once (`HashMap`). The first entry for an id still wins, and the order of patterns, tags and the allowlist did not change.

Measured with the release binary of the last code commit before the fix (`origin/main`, 1a1f415) and of 6778ec7, on the same machine (Linux x86_64, rustc 1.99.0), `cost-check render`:

| Input | Before | After | Report |
|---|---|---|---|
| log-bill JSON, 40 000 patterns with prices | 7.62 s | 0.15 s | byte-identical |
| log-bill JSON, 80 000 patterns with prices | 39.02 s | 0.31 s | byte-identical |
| dd-metric-bill JSON, 60 000 tag keys, all unbounded | 23.17 s | 0.28 s | byte-identical |

New unit tests in `src/adapt.rs`: `many_log_patterns_are_each_priced_from_their_own_id` (40 000 patterns, prices in reverse order), `the_first_price_for_an_id_still_wins`, `a_pattern_without_a_price_shows_its_bytes` and `many_dd_keys_keep_their_order_and_counts` (60 000 keys). They check the result, not the time, so a slow runner cannot turn them red.

## Results

Run on Linux x86_64 on 6778ec7, with rustc 1.99.0 and cargo 1.99.0 (stable) and with rustc 1.85.1 and cargo 1.85.1:

| Command | Result |
|---|---|
| `cargo fmt --check` (stable) | clean, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` (stable) | clean, exit 0 |
| `cargo test --locked` (stable, debug) | 110 passed, 0 failed, 1 ignored, exit 0 |
| `cargo test --release --locked` (stable) | 110 passed, 0 failed, 1 ignored, exit 0 |
| `cargo +1.85 fmt --check` | clean, exit 0 |
| `cargo +1.85 clippy --all-targets --locked -- -D warnings` | clean, exit 0 |
| `cargo +1.85 test --locked` (debug) | 110 passed, 0 failed, 1 ignored, exit 0 |
| `cargo +1.85 test --release --locked` | 110 passed, 0 failed, 1 ignored, exit 0 |

Per test binary, in the debug and release runs on stable and on 1.85:

- 64 unit tests in `src/lib.rs`, all green
- 0 tests in `src/main.rs`
- 44 integration tests in `tests/cli.rs`, all green
- 2 integration tests in `tests/golden.rs`, both green
- 1 test in `tests/live.rs`, ignored on purpose

`tests/live.rs` runs the four real tools. It is ignored in a plain `cargo test`, because a machine without the tools proves nothing and a skipped test must not count as a pass. It was not run (not run).

## Not run

- Windows and macOS: only in the GitHub Actions run of `.github/workflows/ci.yml` on the pull request, not on this machine.
- `cargo deny` and `cargo audit` on this machine: not run.
- `tests/live.rs` with the real tools: not run.
- A CRLF checkout (`core.autocrlf=true`) of the golden fixture: not run.
- Mutation testing, fuzzing and coverage: not run.
