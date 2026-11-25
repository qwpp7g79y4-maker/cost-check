# Kwaliteitsnotities

Korte, eerlijke stand van wat in deze repository getest is en hoe. English version: [QUALITY.md](QUALITY.md).

## Geteste commit

Datum: 2026-11-24. Geteste codecommit: 28de6f3 (tak `chore/cargo-metadata`). Commits daarna wijzigen alleen documenten. Hij herstelt de bevindingen B-1 en B-3 van de eindreview cost-eind-40eebd7 van tip 40eebd7. B-2 (E-N8 in de vorige review) staat nog open en is niet hersteld. De eerdere herstellen (E-B1, E-B2 in 67f7133; B1 tot en met B4 in 317aaf6) zijn ongewijzigd. De bevindingen N-a tot en met N-l en N1 tot en met N13 van de reviews staan nog open.

## Tests

`cargo test -j 2 --locked -- --test-threads=2` op Linux, debugprofiel, rustc 1.99.0, op 28de6f3:

- 60 unit-tests in `src/lib.rs`, allemaal groen
- 0 tests in `src/main.rs`
- 44 integratietests in `tests/cli.rs`, allemaal groen
- 2 integratietests in `tests/golden.rs`, beide groen
- 1 test in `tests/live.rs`, met opzet genegeerd (zie hieronder)
- 106 geslaagd, 0 mislukt, 1 genegeerd.

`cargo fmt --check` en `cargo clippy -j 2 --locked --all-targets -- -D warnings` zijn schoon op Linux.

`tests/live.rs` draait de vier echte tools. Hij wordt in een gewone `cargo test` genegeerd, omdat een machine zonder de tools niets bewijst en een overgeslagen test niet als geslaagd mag tellen. Voor deze commit is hij niet uitgevoerd.

## Rood op de oude tip (B-1, B-3)

De nieuwe tests zijn gedraaid tegen de bron van 40eebd7 (de tests erin gekopieerd, `src/sniff.rs` zoals het was) en zijn daar rood. Dit is de cargo-uitvoer, ingekort tot de regels met een uitslag:

```text
$ cargo test -j 2 --locked --no-fail-fast -- --test-threads=2      (bron van 40eebd7, nieuwe tests)
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

De rode tests, per bevinding:

- B-1: `jsonl_with_no_space_before_the_brace_is_a_log` (unit, `src/sniff.rs`: `{"level":"error","msg":"disk full","host":"a"}` zonder spatie voor de `{` moet `log-bill` met JSONL zijn, en gaf `dd-metric-bill`) en `auto_reads_jsonl_that_starts_with_a_brace_as_a_log` (`tests/cli.rs`, `--tool auto` moet een `log-bill`-rapport geven). De tweede was rood omdat `auto` `dd-metric-bill` koos; de oude bron eindigt in een fout in plaats van een rapport. De sniff-test met een spatie voor de `{` bestond al en blijft groen.
- `a_prometheus_line_with_a_name_stays_prometheus` (`up{job="api"} 1`) is op beide groen: hij bewaakt het geval dat niet mag veranderen.
- B-3: op Linux kon geen test rood gemaakt worden, omdat hier geen CRLF-checkout ontstaat. Het wordt gedekt door het bestand `.gitattributes`, niet door een test. De CRLF-checkout zelf is niet geprobeerd (niet uitgevoerd).

Op 28de6f3 zijn ze allemaal groen (zie Tests).

## Wat het herstel doet

- B-1: `looks_like_prometheus` ziet een lege prefix voor de eerste `{` niet meer als metric-naam (`.all()` op een lege prefix is waar). Een regel met een naam voor de `{` blijft Prometheus. Een expliciete `--tool` had er nooit last van. Verder is er niets aan de herkenning gewijzigd.
- B-3: `.gitattributes` met `* text=auto eol=lf`, zodat de golden-fixture `tests/fixtures/ci.md` in elke checkout alleen nieuweregels heeft. `Cargo.toml` heeft geen `exclude`, dus daar hoefde niets weg. De ingebouwde tarieven en de afrondingsregel zijn niet gewijzigd.

## Open

- B-2 (E-N8 in de vorige review) staat nog open. Het is niet hersteld. `pattern_month` is niet gewijzigd en er is geen `HashMap` voor patronen bijgekomen.

## Eerdere ronde (E-B1, E-B2, 67f7133)

De nieuwe tests zijn gedraaid tegen de bron van 0e32634 (de tests erin gekopieerd, de bron ongewijzigd) en waren daar rood. Dit is de cargo-uitvoer, ingekort tot de regels met een uitslag:

```text
$ cargo test -j 2 --no-fail-fast      (bron van 0e32634, nieuwe tests)
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

De rode tests, per bevinding:

- E-B1: `a_cut_inside_a_multibyte_char_does_not_panic` en `a_state_file_with_a_cut_char_is_still_found` (unit, `src/sniff.rs`: é, 日 en een emoji rond byte 8192, met en zonder BOM), `an_upload_cut_inside_a_multibyte_char_is_a_report` (`src/serve.rs`), `report_survives_a_char_cut_by_the_sniff_window` (auto en expliciete `--tool`, exit niet 101) en `serve_survives_a_char_cut_by_the_sniff_window` (het `serve`-proces moet na de upload nog draaien).
- E-B2: `a_hostile_tool_field_is_one_escaped_line_on_stderr` (een veld `tool` met ESC, BEL en een nieuweregel) en `the_stderr_of_a_failing_tool_is_one_escaped_line` (een tool die ESC, BEL en een nieuweregel naar stderr schrijft). Elk moet één regel geven, zonder stuurteken, die met `cost-check: ` begint.

Op 67f7133 waren ze allemaal groen.

### Wat dat herstel deed

- E-B1: `sniff_prefix` sneed de lossy tekst op byte 8192. Die tekst kan langer zijn dan de bytes (een afgesneden teken wordt U+FFFD, drie bytes), dus het snijpunt kon erin vallen en paniek geven (exit 101, en `serve` stopte). Het snijpunt schuift nu terug naar een tekengrens. De herkenning zelf is niet gewijzigd.
- E-B2: het bericht van een `Fault` gaat door `escape_display` als `main` het print, en de stderr van een falende tool wordt geëscaped (na het afkappen op 500 tekens) voor hij in het bericht komt. Het rapport deed dit al. De ingebouwde tarieven en de afrondingsregel zijn niet gewijzigd.

## Eerdere ronde (B1 tot en met B4, 317aaf6)

De nieuwe en aangepaste tests zijn gedraaid tegen de bron van 749cb16 (de tests erin gekopieerd, de bron ongewijzigd) en zijn daar rood. Dit is de cargo-uitvoer, ingekort tot de regels met een uitslag:

```text
$ cargo test -j 2 --no-fail-fast      (bron van 749cb16, nieuwe tests)
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

De rode tests van die ronde, per bevinding:

- B1: `a_dd_tag_key_cannot_close_the_code_block`, `a_dd_tag_key_cannot_add_a_second_line_after_allow_tags`, `a_rum_type_name_cannot_close_the_code_block`, `a_log_pattern_cannot_add_a_second_line_to_the_drop_rule` en `a_log_pattern_cannot_add_a_tab_to_the_drop_rule`. De eerste en de derde bestonden en eisten de rauwe nieuweregel in het blok; ze eisen nu één regel per voorstel.
- B2: `a_dd_tag_key_is_not_proposed_when_the_shell_would_run_it` (sleutel `env$(id);x`).
- B3: `a_dd_total_counts_the_indexed_line_too` (ingested 900.00 plus indexed 1200.00 gaf 900.00 en geeft nu 2100.00 USD), `a_dd_total_keeps_the_rounding_rule` en `a_dd_total_shows_another_currency_apart`.
- B4: `a_price_file_cannot_replace_the_export_of_the_same_name` (in `src/serve.rs`).

`a_dd_allowlist_of_plain_keys_is_still_proposed` is op beide groen: hij bewaakt het geval dat niet mag veranderen. Op 317aaf6 waren ze allemaal groen.

### Wat dat herstel deed

- B1: een veld uit de export dat in de voorgestelde fix terechtkomt krijgt nieuweregel, regelterugloop, tab en andere stuurtekens als zichtbare escape (`\n`, `\t`), zoals in een cel. De drop-regel van `log-bill` en de typenaam van `rum-meter` gebruiken dit.
- B2: `--allow-tags` wordt alleen voorgesteld als elke sleutel binnen `A-Za-z0-9_.:-` valt en hoogstens 64 tekens heeft (dezelfde controle als de optie). Anders zegt het blok dat er geen allowlist wordt voorgesteld. Er wordt niets gequote, dus geen sleutel kan een shell bereiken.
- B3: `dd-metric-bill` telt alle kostenregels van één valuta op voor het afrondt. Een regel in een andere valuta staat apart. De afrondingsregel is niet gewijzigd en geen ingebouwd tarief is gewijzigd.
- B4: `serve` schrijft de export en het prijsbestand met `unique_path`, zodat een prijsbestand met de naam van de export een andere naam krijgt.

## Platforms

Tests die een stub draaien gebruiken `sh` en een bestandsmodus (`set_mode`), en de test van de modus van de werkmap leest Unix-rechten. Die tests worden alleen op Unix gecompileerd (`#[cfg(unix)]`); op Windows blijven ze weg. De tests die Linux nodig hebben (`/dev/full`, een byte die geen UTF-8 is in een bestandsnaam) worden alleen op Linux gecompileerd.

## Niet uitgevoerd

- De GitHub Actions-run van `.github/workflows/ci.yml` (Windows, macOS, Rust 1.85, `cargo deny`): niet uitgevoerd (not run). Er is niets gepusht.
- `cargo test --release`, `cargo +1.85 test`, de Windows- en macOS-doelen met `cargo check` en `cargo clippy`, `cargo deny` en `cargo audit`: niet uitgevoerd voor 28de6f3. Eerdere uitslagen voor 7a5a3d6 gelden hier niet.
- `tests/live.rs` met de echte tools: niet uitgevoerd.
- Een CRLF-checkout (`core.autocrlf=true`) van de golden-fixture: niet uitgevoerd.
- Geen test telt de koppen van de README-bestanden.
- Open bevindingen B-2, N-a tot en met N-l en N1 tot en met N13: niet hersteld en niet aangeraakt.

## CI

`.github/workflows/ci.yml` heeft drie jobs, bij elke push en pull request. `check` draait `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` en `cargo test --locked` op `ubuntu-latest`, `windows-latest`, `macos-latest` en `macos-15-intel`. `msrv` draait `cargo check --locked --all-targets` en `cargo test --locked` op Rust 1.85. `deny` draait `cargo deny --locked check`. Geen van deze is al op GitHub gedraaid (zie Niet uitgevoerd).
