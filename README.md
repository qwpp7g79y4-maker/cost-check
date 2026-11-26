# Cost Check

[![crates.io](https://img.shields.io/crates/v/cost-check.svg)](https://crates.io/crates/cost-check) [![downloads](https://img.shields.io/crates/d/cost-check.svg)](https://crates.io/crates/cost-check) [![CI](https://github.com/qwpp7g79y4-maker/cost-check/actions/workflows/ci.yml/badge.svg)](https://github.com/qwpp7g79y4-maker/cost-check/actions/workflows/ci.yml) [![PEPAI checked](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fqwpp7g79y4-maker%2Fcost-check%2Fmain%2F.github%2Fbadge.json)](QUALITY.md) [![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://github.com/qwpp7g79y4-maker/cost-check/blob/main/Cargo.toml) [![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/qwpp7g79y4-maker/cost-check#license)

One local report for `ci-bill`, `log-bill`, `dd-metric-bill` and `rum-meter`. Dutch version: [README.nl.md](README.nl.md).

The customer hands over an export. The program writes the report in the shape of `ci-bill report --md`: period, total, the biggest items, findings, and one concrete suggestion. All of that happens on this computer. There is no account and no network: no network crate is linked, `report` and `render` open no connection, and `serve` only listens on a loopback address.

Cost Check starts the existing binary of each tool and reads only its JSON. It does not merge the four tools and does not call `ci-bill fetch`.

## Installation

Rust 1.85 or newer. The package is not published to crates.io, so build it from a checkout of this repository:

```sh
cargo build --release -j 2
```

The binary is `target/release/cost-check`. It also needs the binary of the tool that matches your export (`ci-bill`, `log-bill`, `dd-metric-bill` or `rum-meter`). See Options for how it is found.

## Quick start

```sh
cargo build --release -j 2

target/release/cost-check report --tool dd-metric-bill \
  --input metrics.dogstatsd --out report.md
```

This reads `metrics.dogstatsd`, writes `report.md` and leaves the input alone. Nothing is written into this repository.

## Quickstart in 5 minutes

1. Build the program: `cargo build --release -j 2`.
2. Make sure the binary of the tool for your export can be found, for example on `PATH`.
3. Run `cost-check report --input export.csv --out report.md`. `--tool` can be left out: `auto` recognises the format.
4. Or start `cost-check serve`, open `http://127.0.0.1:4517/` on this computer and choose the export.
5. Read `report.md`. The full Dutch explanation is in [README.nl.md](README.nl.md).

## Options

```text
cost-check report --input FILE [--tool auto|ci-bill|log-bill|dd-metric-bill|rum-meter]
  [--workflows DIR] [--prices FILE] [--window 1m] [--plan pro|enterprise]
  [--hosts N] [--allow-tags a,b] [--hours-per-month N]
  [--format text|jsonl|prometheus|dogstatsd] [--out FILE] [--delete-input]
  [--ci-bill PATH] [--log-bill PATH] [--dd-metric-bill PATH] [--rum-meter PATH]

cost-check render --tool TOOL --json OUTPUT.json [--fix DIFF]

cost-check serve [--bind 127.0.0.1:4517]
```

- `--tool auto` recognises an Actions usage CSV, a Terraform state, DogStatsD, Prometheus text, Datadog JSONL, and otherwise a log. A JSONL line that starts directly with `{` and has no `"metric"` is a log, not Prometheus; Prometheus text needs a metric name before the `{`. It looks only at the first 8 KiB of the export. An explicit `--tool` is never overruled by that guess; only an explicit `--format` that does not fit the tool is refused.
- The path of a tool binary comes from `--ci-bill` and its three siblings, otherwise from `COST_CHECK_CI_BILL`, `COST_CHECK_LOG_BILL`, `COST_CHECK_DD_METRIC_BILL` or `COST_CHECK_RUM_METER`, otherwise from the directory of the `cost-check` program itself, otherwise from `PATH`. A file `NAME.exe` is found as `NAME` on Windows.
- A file given with `--input` stays. `--delete-input` removes only that export, after a successful report. A price file you supply and a workflows directory always stay.
- `--out` writes a new file next to the target and renames it over the target. It refuses a target that is a symlink, and it refuses the same file as `--input` (also through `..` or a hard link): exit 2, and the export is left alone.
- A path that starts with `-` is read as a file, and a path with a byte that is not UTF-8 is passed to the tool unchanged.
- Run `cost-check --help` for the full text of every option.

## Usage

```sh
cost-check report --tool ci-bill --input usage.csv \
  --workflows .github/workflows --out report.md

cost-check report --tool log-bill --input sample.log --window 1m --out report.md

cost-check report --tool rum-meter --input terraform.tfstate --out report.md

cost-check render --tool ci-bill --json ci.json --fix fix.diff

cost-check serve
```

`serve` binds to loopback only. Open `http://127.0.0.1:4517/` on this computer and choose the export. The report appears on the page and the uploaded file is deleted afterwards. A request with a different `Origin` is refused. There is no cookie.

Without a price file the report shows the unit the tool really has (bytes, custom metrics, RUM, series), not an invented amount. With a price file the amount comes from that tool. For `dd-metric-bill` the total adds all cost lines of one currency. The suggested `--allow-tags` line appears only when every key is within `A-Za-z0-9_.:-`; otherwise the report says no allowlist is proposed.

## Example

The report for a sample `ci-bill` export (`tests/fixtures/ci.json`, rendered to `tests/fixtures/ci.md`):

```text
# ci-bill teardown

Period: 2026-09-01 .. 2026-09-30
Repositories: example-org/sample-app

Total net: 284.21 USD

## Top jobs
## Findings
## Suggested fix
```

The tables under each heading keep the columns `Job | File:Line | Amount` and `Finding | File:Line | Message | Savings`. An empty table or a missing suggestion is left out, as in `ci-bill`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | The report was written. |
| 2 | A usage error, an unreadable input, a tool that failed, a refused bind address or `--out`, JSON with a repeated key, or a report that could not be written (also to a full standard output). |

## Limitations

- Offline only. Cost Check never fetches an export and never reads a cloud account.
- It needs the binary of the matching tool; it does not contain the four tools.
- A tool runs for at most 90 seconds. Tool output over 32 MiB is cut off and the tool is stopped. The page accepts at most 32 MiB.
- `serve` handles one request at a time.
- An error on stderr is one line: control characters in the message, in a `tool` field of a JSON file or in the stderr of a failing tool become visible escapes.
- Text from an export is shown safely: control, format and bidi characters become visible escapes (`\x1b`, `\u{202e}`), and `\`, `` ` ``, `<`, `>`, `[` and `]` get a backslash. A bare web address in a cell is still plain text that a viewer may turn into a link.
- A closed standard output (`>&-`) is only detected on Linux, because the Rust runtime replaces it with `/dev/null` at start-up. A full or broken one is an error everywhere.
- The tests that run a stub (`sh`) or read file modes run on Unix only; see [CONTRIBUTING.md](CONTRIBUTING.md).
- The report text is English. Savings in money appear only when the tool's own JSON has them.

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

The licenses of the third-party crates built into the binary are listed in [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html).

## Maintenance

Cost Check is maintained by PEPAI. Report problems through Issues, with a test where you can (see [CONTRIBUTING.md](CONTRIBUTING.md)); security problems go through [SECURITY.md](SECURITY.md). Changes are listed in [CHANGELOG.md](CHANGELOG.md) and the state of testing is in [QUALITY.md](QUALITY.md). Before a change: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test -- --test-threads=2`.

After a release the tool is dormant: a new version appears only for a reported bug, a request, or a security problem. Security always comes first.
