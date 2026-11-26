# Kwaliteitsnotities

pep: niet AF

## Open

| Punt | Klasse | Aftrek |
|---|---|---|
| B-2 staat nog open. Het is niet hersteld. `pattern_month` is niet gewijzigd en er is geen `HashMap` voor patronen bijgekomen. | B- | geen cijfer: niet AF |

Een open B of B- betekent: de tool is niet AF en krijgt geen cijfer.

Korte, eerlijke stand van wat in deze repository getest is en hoe. Engelse versie: [QUALITY.md](QUALITY.md).

## Geteste commit

Datum: 2026-11-25. Geteste codecommit: 45905fc. Commits daarna wijzigen alleen documenten.

45905fc is een reparatie van één functie in `src/money.rs`: clippy van Rust 1.85 keurde `parse_fixed` af (`clippy::comparison_chain`) op de eerdere codecommit 3953b2b, dus de `if`-keten werd een `match` op `Ordering`. Geen `allow` en geen `expect`. Vóór de reparatie was `cargo +1.85 fmt --check` schoon en faalde `cargo +1.85 clippy --all-targets --locked -- -D warnings` met exit 101; `cargo +1.85 test` is op 3953b2b niet uitgevoerd (not run). De stable-uitslagen die op 3953b2b stonden, zijn vervangen door de uitslagen op 45905fc hieronder.

## Uitslag

Gedraaid op Linux x86_64 op 45905fc, met rustc 1.99.0 en cargo 1.99.0 (stable) en met rustc 1.85.1 en cargo 1.85.1:

| Opdracht | Uitslag |
|---|---|
| `cargo fmt --check` (stable) | schoon, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` (stable) | schoon, exit 0 |
| `cargo test --locked` (stable, debug) | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo test --release --locked` (stable) | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo +1.85 fmt --check` | schoon, exit 0 |
| `cargo +1.85 clippy --all-targets --locked -- -D warnings` | schoon, exit 0 |
| `cargo +1.85 test --locked` (debug) | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo +1.85 test --release --locked` | 106 geslaagd, 0 mislukt, 1 genegeerd, exit 0 (2026-11-25) |

Per testbinary, in de debug- en release-run op stable en in de debug-run op 1.85:

- 60 unittests in `src/lib.rs`, allemaal groen
- 0 tests in `src/main.rs`
- 44 integratietests in `tests/cli.rs`, allemaal groen
- 2 integratietests in `tests/golden.rs`, beide groen
- 1 test in `tests/live.rs`, bewust genegeerd

`tests/live.rs` draait de vier echte tools. Hij wordt bij een gewone `cargo test` overgeslagen, omdat een machine zonder de tools niets bewijst en een overgeslagen test niet als geslaagd mag tellen. Hij is niet uitgevoerd (not run).

## Niet uitgevoerd (not run)

- De GitHub Actions-run van `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): niet uitgevoerd (not run). Er is niets gepusht.
- `cargo +1.85 test --release`, de Windows- en macOS-targets, `cargo deny` en `cargo audit`: niet uitgevoerd (not run).
- `tests/live.rs` met de echte tools: niet uitgevoerd (not run).
- Een CRLF-checkout (`core.autocrlf=true`) van de golden fixture: niet uitgevoerd (not run).
- Mutatietests, fuzzing en dekking: niet uitgevoerd (not run).
