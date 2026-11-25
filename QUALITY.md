# Quality notes

Short, honest status of what is tested in this repository and how. Dutch version: [QUALITY.nl.md](QUALITY.nl.md).

## Tested commit

Date: 2026-11-25. Tested code commit: 3953b2b. Commits after it change documents only.

## Results

Run on Linux x86_64, rustc 1.99.0, cargo 1.99.0, on 3953b2b:

| Command | Result |
|---|---|
| `cargo fmt --check` | clean, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` | clean, exit 0 |
| `cargo test --locked` (debug) | 106 passed, 0 failed, 1 ignored, exit 0 |
| `cargo test --release --locked` | 106 passed, 0 failed, 1 ignored, exit 0 |

Per test binary, in both profiles:

- 60 unit tests in `src/lib.rs`, all green
- 0 tests in `src/main.rs`
- 44 integration tests in `tests/cli.rs`, all green
- 2 integration tests in `tests/golden.rs`, both green
- 1 test in `tests/live.rs`, ignored on purpose

`tests/live.rs` runs the four real tools. It is ignored in a plain `cargo test`, because a machine without the tools proves nothing and a skipped test must not count as a pass. It was not run (not run).

## Not run

- The GitHub Actions run of `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): not run (niet uitgevoerd). Nothing was pushed.
- `cargo +1.85 test`, the Windows and macOS targets, `cargo deny` and `cargo audit`: not run.
- `tests/live.rs` with the real tools: not run.
- A CRLF checkout (`core.autocrlf=true`) of the golden fixture: not run.
- Mutation testing, fuzzing and coverage: not run.
