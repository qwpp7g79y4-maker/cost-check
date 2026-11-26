# Kwaliteitsnotities

pep: niet AF

Update 2026-11-26: B-2 is hersteld in 6778ec7 (zie hieronder). De pep-regel hierboven blijft zoals hij is gezet en wordt in deze wijziging niet opnieuw berekend.

Korte, eerlijke stand van wat in deze repository getest is en hoe. Engelse versie: [QUALITY.md](QUALITY.md).

## Geteste commit

Datum: 2026-11-26. Geteste codecommit: 6778ec7. Commits daarna wijzigen alleen documenten.

6778ec7 herstelt B-2. Het log-rapport zocht de prijs van elk patroon op door heel `costs.patterns` te doorlopen, en het dd-rapport deed hetzelfde voor elke tagsleutel (`collect_tags`) en voor de allowlist (`suggest_allow`). De tijd groeide dus met het kwadraat van de invoer. Elke opzoeking gaat nu via één index die één keer wordt gebouwd (`HashMap`). De eerste regel voor een id wint nog steeds, en de volgorde van patronen, tags en allowlist is niet veranderd.

Gemeten met de release-binary van de laatste codecommit vóór de reparatie (`origin/main`, 1a1f415) en van 6778ec7, op dezelfde machine (Linux x86_64, rustc 1.99.0), `cost-check render`:

| Invoer | Voor | Na | Rapport |
|---|---|---|---|
| log-bill-JSON, 40 000 patronen met prijs | 7,62 s | 0,15 s | byte-gelijk |
| log-bill-JSON, 80 000 patronen met prijs | 39,02 s | 0,31 s | byte-gelijk |
| dd-metric-bill-JSON, 60 000 tagsleutels, alle onbegrensd | 23,17 s | 0,28 s | byte-gelijk |

Nieuwe unittests in `src/adapt.rs`: `many_log_patterns_are_each_priced_from_their_own_id` (40 000 patronen, prijzen in omgekeerde volgorde), `the_first_price_for_an_id_still_wins`, `a_pattern_without_a_price_shows_its_bytes` en `many_dd_keys_keep_their_order_and_counts` (60 000 sleutels). Ze controleren de uitkomst, niet de tijd, zodat een trage runner ze niet rood maakt.

## Uitslag

Gedraaid op Linux x86_64 op 6778ec7, met rustc 1.99.0 en cargo 1.99.0 (stable) en met rustc 1.85.1 en cargo 1.85.1:

| Opdracht | Uitslag |
|---|---|
| `cargo fmt --check` (stable) | schoon, exit 0 |
| `cargo clippy --all-targets --locked -- -D warnings` (stable) | schoon, exit 0 |
| `cargo test --locked` (stable, debug) | 110 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo test --release --locked` (stable) | 110 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo +1.85 fmt --check` | schoon, exit 0 |
| `cargo +1.85 clippy --all-targets --locked -- -D warnings` | schoon, exit 0 |
| `cargo +1.85 test --locked` (debug) | 110 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |
| `cargo +1.85 test --release --locked` | 110 geslaagd, 0 mislukt, 1 genegeerd, exit 0 |

Per testbinary, in de debug- en release-run op stable en op 1.85:

- 64 unittests in `src/lib.rs`, allemaal groen
- 0 tests in `src/main.rs`
- 44 integratietests in `tests/cli.rs`, allemaal groen
- 2 integratietests in `tests/golden.rs`, beide groen
- 1 test in `tests/live.rs`, bewust genegeerd

`tests/live.rs` draait de vier echte tools. Hij wordt bij een gewone `cargo test` overgeslagen, omdat een machine zonder de tools niets bewijst en een overgeslagen test niet als geslaagd mag tellen. Hij is niet uitgevoerd (not run).

## Niet uitgevoerd (not run)

- Windows en macOS: alleen in de GitHub Actions-run van `.github/workflows/ci.yml` op de pull request, niet op deze machine.
- `cargo deny` en `cargo audit` op deze machine: niet uitgevoerd (not run).
- `tests/live.rs` met de echte tools: niet uitgevoerd (not run).
- Een CRLF-checkout (`core.autocrlf=true`) van de golden fixture: niet uitgevoerd (not run).
- Mutatietests, fuzzing en dekking: niet uitgevoerd (not run).
