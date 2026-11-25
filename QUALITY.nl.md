# Kwaliteitsnotities

Korte, eerlijke stand van wat in deze repository getest is en hoe. Engelse versie: [QUALITY.md](QUALITY.md).

## Geteste commit

Datum: 2026-11-25. Geteste codecommit: 3953b2b. Commits daarna wijzigen alleen documenten.

## Uitslag

Gedraaid op Linux x86_64, rustc 1.99.0, cargo 1.99.0, op 3953b2b:

| Opdracht | Uitslag |
|---|---|
| `cargo fmt --check` | schoon, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` | schoon, exit 0 |
| `cargo test --locked` (debug) | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo test --release --locked` | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |

Per testbinary, in beide profielen:

- 60 unittests in `src/lib.rs`, allemaal groen
- 0 tests in `src/main.rs`
- 44 integratietests in `tests/cli.rs`, allemaal groen
- 2 integratietests in `tests/golden.rs`, beide groen
- 1 test in `tests/live.rs`, bewust genegeerd

`tests/live.rs` draait de vier echte tools. Hij wordt bij een gewone `cargo test` overgeslagen, omdat een machine zonder de tools niets bewijst en een overgeslagen test niet als geslaagd mag tellen. Hij is niet uitgevoerd (not run).

## Niet uitgevoerd (not run)

- De GitHub Actions-run van `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): niet uitgevoerd (not run). Er is niets gepusht.
- `cargo +1.85 test`, de Windows- en macOS-targets, `cargo deny` en `cargo audit`: niet uitgevoerd (not run).
- `tests/live.rs` met de echte tools: niet uitgevoerd (not run).
- Een CRLF-checkout (`core.autocrlf=true`) van de golden fixture: niet uitgevoerd (not run).
- Mutatietests, fuzzing en dekking: niet uitgevoerd (not run).
