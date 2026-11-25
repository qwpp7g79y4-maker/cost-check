# Contributing

Contributions go through Issues.

1. Open an Issue first. Describe the export or command, what you expected and what Cost Check did. Use made-up values, not real billing data.
2. Add a test. A bug report comes with a failing test, or the smallest input that reproduces it; a change comes with a test that covers it. Integration tests live in `tests/`, fixtures in `tests/fixtures/`.
3. Keep the change small and run, with few parallel jobs:

   ```sh
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test -j 2 -- --test-threads=2
   ```

Tests that run a stub tool (an `sh` script) or read file modes, such as the test of the mode of the work folder, run on Unix only and are left out on Windows with `#[cfg(unix)]`. Tests that need `/dev/full` or a file name that is not UTF-8 run on Linux only. Check that Windows still compiles with `cargo check --locked --tests --target x86_64-pc-windows-gnu`.

`tests/live.rs` needs the four real tools, so it is ignored in a plain `cargo test`. Run it on purpose with the tool paths in `COST_CHECK_CI_BILL`, `COST_CHECK_LOG_BILL`, `COST_CHECK_DD_METRIC_BILL` and `COST_CHECK_RUM_METER`, the samples folder of `ci-bill` in `COST_CHECK_CI_BILL_SAMPLES`, and `cargo test --test live -- --ignored`.

Cost Check stays offline: no network crate, no account, no cookie. A change that adds one will not be accepted. Security problems do not belong in a public Issue; see [SECURITY.md](SECURITY.md).

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of the project.

Nederlands: meld een probleem of wens als Issue, met een test erbij.
