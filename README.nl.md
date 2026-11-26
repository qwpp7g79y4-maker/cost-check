# Cost Check

[![crates.io](https://img.shields.io/crates/v/cost-check.svg)](https://crates.io/crates/cost-check) [![downloads](https://img.shields.io/crates/d/cost-check.svg)](https://crates.io/crates/cost-check) [![CI](https://github.com/qwpp7g79y4-maker/cost-check/actions/workflows/ci.yml/badge.svg)](https://github.com/qwpp7g79y4-maker/cost-check/actions/workflows/ci.yml) [![PEPAI checked](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fqwpp7g79y4-maker%2Fcost-check%2Fmain%2F.github%2Fbadge.json)](QUALITY.md) [![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://github.com/qwpp7g79y4-maker/cost-check/blob/main/Cargo.toml) [![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/qwpp7g79y4-maker/cost-check#license)

Eén lokaal rapport voor `ci-bill`, `log-bill`, `dd-metric-bill` en `rum-meter`. English version: [README.md](README.md).

De klant levert een export aan. Het programma schrijft het rapport in de vorm van `ci-bill report --md`: periode, totaal, de grootste posten, bevindingen en een concreet voorstel. Dat gebeurt op deze computer. Er is geen account en geen netwerk: er zit geen netwerk-crate in, `report` en `render` maken geen verbinding en `serve` luistert alleen op een loopback-adres.

## Installatie

Rust 1.85 of nieuwer. Het pakket staat niet op crates.io; bouw het uit een checkout van deze repository:

```sh
cargo build --release -j 2
```

De binary is `target/release/cost-check`. Daarnaast heb je de binary nodig van de tool die bij je export hoort (`ci-bill`, `log-bill`, `dd-metric-bill` of `rum-meter`).

## Aan de slag in 5 minuten

1. Bouw het programma: `cargo build --release -j 2`.
2. Zorg dat de binary van de tool bij je export bereikbaar is, bijvoorbeeld op `PATH`.
3. Draai `cost-check report --input export.csv --out rapport.md`. `--tool` mag weg: `auto` herkent het formaat.
4. Of start `cost-check serve`, open `http://127.0.0.1:4517/` op deze computer en kies de export.
5. Lees `rapport.md`. Je export blijft staan, tenzij je `--delete-input` zet.

## Opties

```text
cost-check report --input BESTAND [--tool auto|ci-bill|log-bill|dd-metric-bill|rum-meter]
  [--workflows MAP] [--prices BESTAND] [--window 1m] [--plan pro|enterprise]
  [--hosts N] [--allow-tags a,b] [--hours-per-month N]
  [--format text|jsonl|prometheus|dogstatsd] [--out BESTAND] [--delete-input]
  [--ci-bill PAD] [--log-bill PAD] [--dd-metric-bill PAD] [--rum-meter PAD]

cost-check render --tool TOOL --json UITVOER.json [--fix DIFF]

cost-check serve [--bind 127.0.0.1:4517]
```

- Het pad naar een tool komt van `--ci-bill` en de drie verwante vlaggen, anders van `COST_CHECK_CI_BILL`, `COST_CHECK_LOG_BILL`, `COST_CHECK_DD_METRIC_BILL` of `COST_CHECK_RUM_METER`, anders uit de map van het `cost-check`-programma zelf, anders van `PATH`. Op Windows wordt `NAAM.exe` gevonden als `NAAM`.
- Een bestand dat je met `--input` opgeeft blijft staan. `--delete-input` wist alleen die export, na een gelukt rapport. Een prijsbestand en een workflows-map blijven altijd staan.
- Een JSONL-regel die direct met `{` begint en geen `"metric"` heeft, is voor `--tool auto` een log en geen Prometheus; Prometheus-tekst heeft een metric-naam voor de `{` nodig.
- `--tool auto` kijkt alleen naar de eerste 8 KiB van de export. Een expliciete `--tool` wordt nooit door die gok overruled; alleen een expliciete `--format` die niet bij de tool past wordt geweigerd.
- `--out` schrijft een nieuw bestand naast het doel en hernoemt het over het doel. Een doel dat een symlink is wordt geweigerd, en hetzelfde bestand als `--input` ook (ook via `..` of een hardlink): afsluitcode 2, de export blijft staan.
- Een pad dat met `-` begint is een bestand, en een pad met een byte die geen UTF-8 is gaat ongewijzigd naar de tool.

## Gebruik

```sh
cost-check report --tool ci-bill --input usage.csv \
  --workflows .github/workflows --out rapport.md

cost-check report --tool log-bill --input sample.log --window 1m --out rapport.md

cost-check serve
```

Bij `serve` open je `http://127.0.0.1:4517/` op deze computer en kies je de export. Het rapport verschijnt op de pagina en het aangeleverde bestand wordt daarna gewist. Een verzoek met een ander `Origin` wordt geweigerd. Er is geen cookie.

Zonder prijsbestand toont het rapport de eenheid die de tool echt heeft (bytes, custom metrics, RUM, series), geen verzonnen bedrag. Bij `dd-metric-bill` telt het totaal alle kostenregels van één valuta op. Het voorstel `--allow-tags` verschijnt alleen als elke sleutel binnen `A-Za-z0-9_.:-` valt; anders zegt het rapport dat er geen allowlist wordt voorgesteld.

## Voorbeeld

Het rapport bij de voorbeeldexport van `ci-bill` (`tests/fixtures/ci.json`, weergegeven in `tests/fixtures/ci.md`) heeft deze regels, in deze volgorde:

```text
# ci-bill teardown

Period: 2026-09-01 .. 2026-09-30
Repositories: example-org/sample-app

Total net: 284.21 USD

## Top jobs
## Findings
## Suggested fix
```

Het binnenwerk is Engels, zoals het voorbeeldrapport. Een lege tabel of een ontbrekend voorstel valt weg.

## Afsluitcodes

| Code | Betekenis |
| --- | --- |
| 0 | Het rapport is geschreven. |
| 2 | Verkeerd gebruik, onleesbare invoer, een tool die faalde, een geweigerd bind-adres of `--out`, JSON met een dubbele sleutel, of een rapport dat niet geschreven kon worden (ook naar een volle stdout). |

## Beperkingen

- Alleen offline. Cost Check haalt nooit een export op en leest nooit een cloudaccount.
- Het heeft de binary van de passende tool nodig; de vier tools zitten er niet in.
- Een tool draait hoogstens 90 seconden. Uitvoer boven 32 MiB wordt afgekapt en de tool gestopt. De pagina neemt hoogstens 32 MiB aan.
- `serve` handelt één verzoek tegelijk af.
- Een foutmelding op stderr is één regel: stuurtekens in het bericht, in een veld `tool` van een JSON-bestand of in de stderr van een falende tool worden zichtbare escapes.
- Tekst uit een export wordt veilig getoond: stuur-, opmaak- en bidi-tekens worden zichtbare escapes (`\x1b`, `\u{202e}`), en `\`, `` ` ``, `<`, `>`, `[` en `]` krijgen een backslash. Een los webadres in een cel blijft gewone tekst die een viewer als link kan tonen.
- Een dichte stdout (`>&-`) wordt alleen op Linux herkend, omdat de Rust-runtime hem bij de start vervangt door `/dev/null`. Een volle of afgebroken stdout is overal een fout.
- De tests die een stub (`sh`) draaien of bestandsmodi lezen draaien alleen op Unix; zie [CONTRIBUTING.md](CONTRIBUTING.md).

## Licentie

MIT OR Apache-2.0, naar keuze. Zie [LICENSE-MIT](LICENSE-MIT) en [LICENSE-APACHE](LICENSE-APACHE).

De licenties van de externe crates in het programma staan in [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html).

## Onderhoud

Cost Check wordt onderhouden door PEPAI. Meld problemen via Issues, met een test waar dat kan (zie [CONTRIBUTING.md](CONTRIBUTING.md)); beveiligingsproblemen gaan via [SECURITY.md](SECURITY.md). Wijzigingen staan in [CHANGELOG.md](CHANGELOG.md) en de teststand in [QUALITY.nl.md](QUALITY.nl.md). Voor een wijziging: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` en `cargo test -- --test-threads=2`.

Na een release staat de tool in sluimermodus (dormant): een nieuwe versie komt er alleen bij een gemelde bug, een verzoek of een beveiligingsprobleem. Beveiliging gaat altijd voor.
