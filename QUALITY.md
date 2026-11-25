# Quality notes

Short, honest status of what is tested in this repository and how. Dutch version: [QUALITY.nl.md](QUALITY.nl.md).

## Tested commit

Date: 2026-11-25. Tested code commit: 45905fc. Commits after it change documents only.

45905fc is a one-function fix in `src/money.rs`: Rust 1.85 clippy rejected `parse_fixed` (`clippy::comparison_chain`) on the earlier code commit 3953b2b, so the `if` chain became a `match` on `Ordering`. No `allow` and no `expect`. Before the fix, `cargo +1.85 fmt --check` was clean and `cargo +1.85 clippy --all-targets --locked -- -D warnings` failed with exit 101; `cargo +1.85 test` was not run on 3953b2b. The stable results recorded on 3953b2b are replaced by the results on 45905fc below.

## Results

Run on Linux x86_64 on 45905fc, with rustc 1.99.0 and cargo 1.99.0 (stable) and with rustc 1.85.1 and cargo 1.85.1:

| Command | Result |
|---|---|
| `cargo fmt --check` (stable) | clean, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` (stable) | clean, exit 0 |
| `cargo test --locked` (stable, debug) | 106 passed, 0 failed, 1 ignored, exit 0 |
| `cargo test --release --locked` (stable) | 106 passed, 0 failed, 1 ignored, exit 0 |
| `cargo +1.85 fmt --check` | clean, exit 0 |
| `cargo +1.85 clippy --all-targets --locked -- -D warnings` | clean, exit 0 |
| `cargo +1.85 test --locked` (debug) | 106 passed, 0 failed, 1 ignored, exit 0 |
| `cargo +1.85 test --release --locked` | not run |

Per test binary, in the debug and release runs on stable and in the debug run on 1.85:

- 60 unit tests in `src/lib.rs`, all green
- 0 tests in `src/main.rs`
- 44 integration tests in `tests/cli.rs`, all green
- 2 integration tests in `tests/golden.rs`, both green
- 1 test in `tests/live.rs`, ignored on purpose

`tests/live.rs` runs the four real tools. It is ignored in a plain `cargo test`, because a machine without the tools proves nothing and a skipped test must not count as a pass. It was not run (not run).

## Not run

- The GitHub Actions run of `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): not run (niet uitgevoerd). Nothing was pushed.
- `cargo +1.85 test --release`, the Windows and macOS targets, `cargo deny` and `cargo audit`: not run.
- `tests/live.rs` with the real tools: not run.
- A CRLF checkout (`core.autocrlf=true`) of the golden fixture: not run.
- Mutation testing, fuzzing and coverage: not run.
