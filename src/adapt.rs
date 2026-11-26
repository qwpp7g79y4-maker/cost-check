//! JSON from each tool, filled into the one report.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::model::{finding_headers, top_headers, Teardown};
use crate::money::{self, display_amount, format_cents_str, proportion};
use crate::sniff::Tool;
use crate::Fault;

pub fn teardown(tool: Tool, json: &Value, fix: Option<&str>) -> Result<Teardown, Fault> {
    match tool {
        Tool::Ci => ci(json, fix),
        Tool::Log => log_report(json),
        Tool::Dd => dd(json),
        Tool::Rum => rum(json),
    }
}

fn ci(json: &Value, fix: Option<&str>) -> Result<Teardown, Fault> {
    expect_schema(json, "ci-bill", 2)?;
    let currency = json
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("USD");
    let net = json
        .get("total_net")
        .and_then(Value::as_str)
        .ok_or_else(|| Fault::new("ci-bill JSON mist total_net"))?;
    let total = format!("{} {currency}", format_cents_str(net)?);
    let mut jobs = Vec::new();
    let mut found = Vec::new();
    for workflow in workflows(json) {
        for job in array(workflow, "job_costs") {
            let amount = job
                .get("amount")
                .and_then(Value::as_str)
                .ok_or_else(|| Fault::new("ci-bill JSON mist een job-bedrag"))?;
            let micros = money::parse_fixed(amount, 6)?;
            let file = job
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let line = job.get("line").and_then(Value::as_u64);
            let name = job
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let job_id = job.get("job_id").and_then(Value::as_u64);
            jobs.push(CiJob {
                name,
                file,
                line,
                job_id,
                micros,
            });
        }
        push_findings(&mut found, array(workflow, "findings"))?;
    }
    push_findings(&mut found, array(json, "unattached_findings"))?;
    jobs.sort_by(|left, right| {
        right
            .micros
            .cmp(&left.micros)
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.name.cmp(&right.name))
    });
    jobs.truncate(5);
    found.sort_by(|left, right| {
        right
            .savings
            .cmp(&left.savings)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.rank.cmp(&right.rank))
            .then_with(|| left.message.cmp(&right.message))
    });
    let top_rows = jobs
        .iter()
        .map(|job| {
            let shown = if job.name.is_empty() {
                "job".to_string()
            } else {
                job.name.clone()
            };
            vec![shown, job_location(job), cents_from(job.micros)]
        })
        .collect();
    let findings = found
        .iter()
        .map(|item| {
            vec![
                item.id.clone(),
                format!("{}:{}", item.path, item.line),
                item.message.clone(),
                cents_from(item.savings),
            ]
        })
        .collect();
    let unit = currency.to_string();
    Ok(page(
        Tool::Ci,
        Shell {
            period: period_ci(json),
            scope_label: "Repositories",
            scope: repos(json),
            total,
            note: None,
        },
        Columns {
            top_unit: unit.clone(),
            top_rows,
            finding_unit: unit,
            findings,
            fix_lang: "diff",
            fix_body: fix.map(str::to_string),
        },
    ))
}

struct CiJob {
    name: String,
    file: String,
    line: Option<u64>,
    job_id: Option<u64>,
    micros: i128,
}

struct CiFinding {
    savings: i128,
    path: String,
    line: u64,
    rank: u8,
    id: String,
    message: String,
}

fn job_location(job: &CiJob) -> String {
    if !job.file.is_empty() {
        if let Some(line) = job.line {
            return format!("{}:{line}", job.file);
        }
    }
    match job.job_id {
        Some(id) => format!("job {id}"),
        None => "-".to_string(),
    }
}

fn cents_from(micros: i128) -> String {
    money::cents_from_micros(micros)
}

fn push_findings(out: &mut Vec<CiFinding>, items: &[Value]) -> Result<(), Fault> {
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("F")
            .to_string();
        let savings = item
            .get("savings")
            .and_then(Value::as_str)
            .ok_or_else(|| Fault::new("ci-bill JSON mist een besparing"))?;
        out.push(CiFinding {
            savings: money::parse_fixed(savings, 6)?,
            path: item
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            line: item.get("line").and_then(Value::as_u64).unwrap_or(0),
            rank: kind_rank(&id),
            id,
            message: item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(())
}

fn kind_rank(id: &str) -> u8 {
    match id {
        "F1" => 0,
        "F2" => 1,
        "F3" => 2,
        "F4" => 3,
        "F8" => 4,
        "F9" => 5,
        "F6" => 6,
        "F7" => 7,
        "F10" => 8,
        "F11" => 9,
        "F12" => 10,
        "F13" => 11,
        "F5" => 12,
        "F3$" => 13,
        _ => 14,
    }
}

fn period_ci(json: &Value) -> String {
    let Some(period) = json.get("period").filter(|value| !value.is_null()) else {
        return "(none)".to_string();
    };
    match (
        period.get("from").and_then(Value::as_str),
        period.get("to").and_then(Value::as_str),
    ) {
        (Some(from), Some(to)) => format!("{from} .. {to}"),
        _ => "(none)".to_string(),
    }
}

fn repos(json: &Value) -> String {
    let names: Vec<&str> = array(json, "repositories")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

fn workflows(json: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    out.extend(array(json, "workflows"));
    if let Some(outside) = json.get("outside_top") {
        out.extend(array(outside, "workflows"));
    }
    out
}

fn log_report(json: &Value) -> Result<Teardown, Fault> {
    expect_schema(json, "log-bill", 1)?;
    expect_tool(json, "log-bill")?;
    let paths = json
        .pointer("/inputs/paths")
        .map(strings)
        .unwrap_or_default();
    let scope = if paths.is_empty() {
        "(none)".to_string()
    } else {
        paths.join(", ")
    };
    let period = match json.get("window").filter(|value| !value.is_null()) {
        Some(window) => {
            let seconds = window.get("seconds").and_then(Value::as_u64).unwrap_or(0);
            match window.get("factor").and_then(Value::as_str) {
                Some(factor) if !factor.is_empty() => {
                    format!("sample ({seconds}s, factor {factor})")
                }
                _ => format!("sample ({seconds}s)"),
            }
        }
        None => "sample".to_string(),
    };
    let currency = json.pointer("/costs/currency").and_then(Value::as_str);
    let month = json
        .pointer("/costs/totals/ingest/per_month")
        .and_then(Value::as_str);
    let (total, unit) = if let (Some(currency), Some(month)) = (currency, month) {
        (
            format!("{} {currency}", display_amount(month)?),
            currency.to_string(),
        )
    } else {
        let lines = json
            .pointer("/totals/lines")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let bytes = json
            .pointer("/totals/bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        (format!("{lines} lines, {bytes} bytes"), "bytes".to_string())
    };
    let prices = price_index(json);
    let mut patterns = Vec::new();
    for pattern in array(json, "patterns") {
        let id = pattern
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let text = pattern
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let bytes = pattern.get("bytes").and_then(Value::as_u64).unwrap_or(0);
        let share = pattern
            .get("share")
            .and_then(Value::as_str)
            .unwrap_or("0.0");
        let share = if share.contains('%') {
            share.to_string()
        } else {
            format!("{share}%")
        };
        let rest = pattern
            .get("rest")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let priced = pattern_month(&prices, &id)?;
        patterns.push(LogPattern {
            id,
            text,
            bytes,
            share,
            rest,
            priced,
        });
    }
    patterns.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.text.cmp(&right.text))
    });
    let ranked: Vec<&LogPattern> = patterns.iter().take(5).collect();
    let top_rows = ranked
        .iter()
        .map(|pattern| {
            let name = if pattern.rest {
                "other".to_string()
            } else {
                shorten(&pattern.text, 48)
            };
            let amount = match &pattern.priced {
                Some(amount) => amount.clone(),
                None => pattern.bytes.to_string(),
            };
            vec![name, pattern.id.clone(), amount]
        })
        .collect();
    let findings = ranked
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            let savings = match &pattern.priced {
                Some(amount) => amount.clone(),
                None => pattern.bytes.to_string(),
            };
            vec![
                format!("L{}", index + 1),
                pattern.id.clone(),
                shorten(&format!("{} {}", pattern.share, pattern.text), 160),
                savings,
            ]
        })
        .collect();
    let fix = patterns
        .iter()
        .find(|pattern| !pattern.rest)
        .and_then(|pattern| {
            if pattern.text.is_empty() || pattern.text.contains("```") {
                return None;
            }
            let regex = mask_to_regex(&pattern.text);
            if regex.is_empty() {
                None
            } else {
                Some(format!("drop {regex}"))
            }
        });
    let note = json
        .get("estimate")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    Ok(page(
        Tool::Log,
        Shell {
            period,
            scope_label: "Inputs",
            scope,
            total,
            note,
        },
        Columns {
            top_unit: unit.clone(),
            top_rows,
            finding_unit: unit,
            findings,
            fix_lang: "text",
            fix_body: fix,
        },
    ))
}

struct LogPattern {
    id: String,
    text: String,
    bytes: u64,
    share: String,
    rest: bool,
    priced: Option<String>,
}

/// Each id in `costs.patterns` once, mapped to its first entry. A later entry
/// with the same id is ignored, as before, so the first one still wins.
fn price_index(json: &Value) -> HashMap<&str, &Value> {
    let mut index = HashMap::new();
    if let Some(patterns) = json.pointer("/costs/patterns").and_then(Value::as_array) {
        for pattern in patterns {
            if let Some(id) = pattern.get("id").and_then(Value::as_str) {
                index.entry(id).or_insert(pattern);
            }
        }
    }
    index
}

fn pattern_month(prices: &HashMap<&str, &Value>, id: &str) -> Result<Option<String>, Fault> {
    let Some(pattern) = prices.get(id) else {
        return Ok(None);
    };
    let Some(raw) = pattern.pointer("/ingest/per_month").and_then(Value::as_str) else {
        return Ok(None);
    };
    Ok(Some(display_amount(raw)?))
}

fn mask_to_regex(text: &str) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < text.len() {
        if text.as_bytes()[index] == b'<' {
            if let Some(end) = text[index..].find('>') {
                let token = &text[index..index + end + 1];
                if is_mask(token) {
                    out.push_str(mask_regex(token));
                    index += end + 1;
                    continue;
                }
            }
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push_str(&regex_escape(ch));
        index += ch.len_utf8();
    }
    out
}

fn is_mask(token: &str) -> bool {
    let Some(inner) = token
        .strip_prefix('<')
        .and_then(|text| text.strip_suffix('>'))
    else {
        return false;
    };
    let mut chars = inner.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {
            chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        }
        _ => false,
    }
}

fn mask_regex(token: &str) -> &'static str {
    if token == "<NUM>" {
        r"\d+"
    } else {
        r"\S+"
    }
}

fn regex_escape(ch: char) -> String {
    match ch {
        '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' => {
            format!("\\{ch}")
        }
        // A line break or tab from the export would split the one-line rule.
        c => crate::model::escape_display(&c.to_string()),
    }
}

fn dd(json: &Value) -> Result<Teardown, Fault> {
    expect_schema(json, "dd-metric-bill", 1)?;
    expect_tool(json, "dd-metric-bill")?;
    let total_n = json
        .get("total")
        .and_then(Value::as_u64)
        .ok_or_else(|| Fault::new("dd-metric-bill JSON is geen count (total is geen getal)"))?;
    let scope = join_inputs(json);
    let money_line = dd_money(json)?;
    let (total, note_extra) = match &money_line {
        Some(parts) => (
            parts
                .iter()
                .map(|(amount, currency)| format!("{amount} {currency}"))
                .collect::<Vec<_>>()
                .join(", "),
            Some(format!("{total_n} custom metrics")),
        ),
        None => (format!("{total_n} custom metrics"), None),
    };
    let mut metrics = Vec::new();
    for metric in array(json, "metrics") {
        let name = metric
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        metrics.push((
            name,
            metric
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            metric.get("count").and_then(Value::as_u64).unwrap_or(0),
        ));
    }
    metrics.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    metrics.truncate(5);
    let top_rows = metrics
        .iter()
        .map(|(name, kind, count)| vec![name.clone(), kind.clone(), count.to_string()])
        .collect();
    let tags = collect_tags(json);
    let dropped = tags_to_drop(&tags);
    let allow = suggest_allow(&tags, &dropped);
    let mut findings = Vec::new();
    for (index, tag) in dropped.iter().enumerate() {
        let message = if tag.reasons.is_empty() {
            format!("tag {} adds {} series", tag.key, tag.added)
        } else {
            format!(
                "unbounded tag {} ({}); it adds {} series",
                tag.key,
                tag.reasons.join(", "),
                tag.added
            )
        };
        findings.push(vec![
            format!("T{}", index + 1),
            format!("{} {}", tag.metric, tag.key),
            message,
            tag.added.to_string(),
        ]);
    }
    let fix = match allow {
        // The line may be pasted into a shell, and a key comes from the
        // export: only keys the tool itself accepts (`valid_tags`) are
        // proposed. Any other key leaves no proposal, not a quoted one.
        Some(list) if crate::run::valid_tags(&list) => Some(format!("--allow-tags {list}")),
        Some(_) => Some(
            "no allowlist proposed: a tag key has characters outside A-Z a-z 0-9 _ . : - or is longer than 64"
                .to_string(),
        ),
        None if !dropped.is_empty() => Some(
            "every removable tag was dropped; an allowlist would index nothing in this sample"
                .to_string(),
        ),
        None => None,
    };
    let estimate = json.get("estimate").and_then(Value::as_str);
    Ok(page(
        Tool::Dd,
        Shell {
            period: "sample".to_string(),
            scope_label: "Inputs",
            scope,
            total,
            note: combine_note(estimate, note_extra),
        },
        Columns {
            top_unit: "custom metrics".to_string(),
            top_rows,
            finding_unit: "series".to_string(),
            findings,
            fix_lang: "text",
            fix_body: fix,
        },
    ))
}

struct Tag {
    key: String,
    added: u64,
    unbounded: bool,
    reasons: Vec<String>,
    metric: String,
}

/// One tag key as one metric uses it. The same key in two metrics is two
/// entries: each keeps its own series count, its own bounds and its own name.
fn collect_tags(json: &Value) -> Vec<Tag> {
    let mut tags: Vec<Tag> = Vec::new();
    // (metric, key) to its place in `tags`, so a lookup does not scan the list.
    let mut places: HashMap<(String, String), usize> = HashMap::new();
    for metric in array(json, "tags") {
        let metric_name = metric.get("name").and_then(Value::as_str).unwrap_or("");
        for key in array(metric, "keys") {
            let Some(name) = key.get("key").and_then(Value::as_str) else {
                continue;
            };
            let added = key.get("added").and_then(Value::as_u64).unwrap_or(0);
            if let Some(existing) = entry(&mut tags, &places, metric_name, name) {
                existing.added = existing.added.saturating_add(added);
            } else {
                places.insert((metric_name.to_string(), name.to_string()), tags.len());
                tags.push(Tag {
                    key: name.to_string(),
                    added,
                    unbounded: false,
                    reasons: Vec::new(),
                    metric: metric_name.to_string(),
                });
            }
        }
    }
    for metric in array(json, "unbounded") {
        let metric_name = metric.get("name").and_then(Value::as_str).unwrap_or("");
        for key in array(metric, "keys") {
            let Some(name) = key.get("key").and_then(Value::as_str) else {
                continue;
            };
            let reasons: Vec<String> = key
                .get("reasons")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(existing) = entry(&mut tags, &places, metric_name, name) {
                existing.unbounded = true;
                for reason in reasons {
                    if !existing.reasons.contains(&reason) {
                        existing.reasons.push(reason);
                    }
                }
            } else {
                places.insert((metric_name.to_string(), name.to_string()), tags.len());
                tags.push(Tag {
                    key: name.to_string(),
                    added: 0,
                    unbounded: true,
                    reasons,
                    metric: metric_name.to_string(),
                });
            }
        }
    }
    tags
}

fn entry<'a>(
    tags: &'a mut [Tag],
    places: &HashMap<(String, String), usize>,
    metric: &str,
    key: &str,
) -> Option<&'a mut Tag> {
    let place = *places.get(&(metric.to_string(), key.to_string()))?;
    tags.get_mut(place)
}

/// The entries to report. An allowlist works on key names, so a key that is
/// unbounded in one metric leaves the allowlist for every metric, but the
/// finding names the metric where it is unbounded and counts only its series.
fn tags_to_drop(tags: &[Tag]) -> Vec<Tag> {
    let removable: Vec<&Tag> = tags.iter().filter(|tag| tag.key != "host").collect();
    if removable.is_empty() {
        return Vec::new();
    }
    let unbounded: Vec<Tag> = removable
        .iter()
        .filter(|tag| tag.unbounded)
        .map(|tag| clone_tag(tag))
        .collect();
    if !unbounded.is_empty() {
        return unbounded;
    }
    let best = removable.iter().min_by(|left, right| {
        right
            .added
            .cmp(&left.added)
            .then_with(|| left.key.cmp(&right.key))
            .then_with(|| left.metric.cmp(&right.metric))
    });
    best.map(|tag| vec![clone_tag(tag)]).unwrap_or_default()
}

fn clone_tag(tag: &Tag) -> Tag {
    Tag {
        key: tag.key.clone(),
        added: tag.added,
        unbounded: tag.unbounded,
        reasons: tag.reasons.clone(),
        metric: tag.metric.clone(),
    }
}

fn suggest_allow(tags: &[Tag], dropped: &[Tag]) -> Option<String> {
    if dropped.is_empty() {
        return None;
    }
    let dropped_keys: HashSet<&str> = dropped.iter().map(|drop| drop.key.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut kept: Vec<&str> = Vec::new();
    for tag in tags {
        let key = tag.key.as_str();
        if !dropped_keys.contains(key) && seen.insert(key) {
            kept.push(key);
        }
    }
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(","))
    }
}

/// Every cost line of the export, added up per currency. A line without its
/// own currency uses the currency of `costs`. Each total keeps the display
/// rule of `display_amount`; sums are made in micros before rounding.
fn dd_money(json: &Value) -> Result<Option<Vec<(String, String)>>, Fault> {
    let Some(costs) = json.get("costs").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let default = costs
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("USD");
    let mut sums: Vec<(String, i128)> = Vec::new();
    for line in array(costs, "lines") {
        let Some(raw) = line.get("amount").and_then(Value::as_str) else {
            continue;
        };
        let currency = line
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or(default);
        let micros = crate::money::parse_fixed(raw, 6)?;
        match sums.iter_mut().find(|(name, _)| name == currency) {
            Some((_, sum)) => {
                *sum = sum
                    .checked_add(micros)
                    .ok_or_else(|| Fault::new("geen bedrag"))?;
            }
            None => sums.push((currency.to_string(), micros)),
        }
    }
    if sums.is_empty() {
        return Ok(None);
    }
    let mut out = Vec::new();
    for (currency, micros) in sums {
        let amount = display_amount(&crate::money::format_fixed(micros, 6))?;
        out.push((amount, currency));
    }
    Ok(Some(out))
}

fn rum(json: &Value) -> Result<Teardown, Fault> {
    expect_schema(json, "rum-meter", 1)?;
    expect_tool(json, "rum-meter")?;
    let total_rum = json
        .get("total_rum")
        .and_then(Value::as_u64)
        .ok_or_else(|| Fault::new("rum-meter JSON mist total_rum"))?;
    let scope = join_inputs(json);
    let costs = array(json, "costs");
    let first = costs.first();
    let tier_name = first
        .and_then(|cost| cost.get("name").and_then(Value::as_str))
        .unwrap_or("");
    let tier_amount = first.and_then(|cost| cost.get("amount").and_then(Value::as_str));
    let tier_currency = first
        .and_then(|cost| cost.get("currency").and_then(Value::as_str))
        .unwrap_or("USD");
    let priced = tier_amount.is_some();
    let unit = if priced {
        tier_currency.to_string()
    } else {
        "RUM".to_string()
    };
    let total = if costs.is_empty() {
        format!("{total_rum} RUM")
    } else {
        let mut parts = Vec::new();
        for cost in costs {
            let amount = cost.get("amount").and_then(Value::as_str).unwrap_or("0");
            let currency = cost.get("currency").and_then(Value::as_str).unwrap_or("");
            let name = cost.get("name").and_then(Value::as_str).unwrap_or("");
            parts.push(format!("{} {currency} ({name})", display_amount(amount)?));
        }
        parts.join(", ")
    };
    let mut modules = rows(json, "by_module");
    modules.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    modules.truncate(5);
    let top_rows = modules
        .iter()
        .map(|(name, count)| {
            let amount = rum_amount(tier_amount, *count, total_rum)?;
            Ok(vec![name.clone(), "module".to_string(), amount])
        })
        .collect::<Result<Vec<_>, Fault>>()?;
    let mut types = rows(json, "by_type");
    types.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let shown = types.len().min(12);
    let mut findings = Vec::new();
    for (index, (name, count)) in types.iter().take(shown).enumerate() {
        let savings = rum_amount(tier_amount, *count, total_rum)?;
        let message = if priced {
            format!("{count} of {total_rum} RUM, tier {tier_name}")
        } else {
            format!("{count} of {total_rum} RUM")
        };
        findings.push(vec![
            format!("T{}", index + 1),
            name.clone(),
            message,
            savings,
        ]);
    }
    let fix = types.first().map(|(name, count)| {
        format!(
            "Largest billed type: {}\n{count} of {total_rum} RUM\nA removed managed resource of this type lowers the hourly peak.",
            crate::model::escape_display(name)
        )
    });
    let estimate = json
        .get("estimate")
        .and_then(Value::as_str)
        .map(as_estimate);
    let note = if priced {
        combine_note(estimate.as_deref(), Some(format!("{total_rum} RUM")))
    } else {
        estimate
    };
    Ok(page(
        Tool::Rum,
        Shell {
            period: "sample".to_string(),
            scope_label: "State",
            scope,
            total,
            note,
        },
        Columns {
            top_unit: unit.clone(),
            top_rows,
            finding_unit: unit,
            findings,
            fix_lang: "text",
            fix_body: fix,
        },
    ))
}

fn rum_amount(tier: Option<&str>, count: u64, total: u64) -> Result<String, Fault> {
    match tier {
        Some(amount) => proportion(amount, count, total),
        None => Ok(count.to_string()),
    }
}

fn rows(json: &Value, key: &str) -> Vec<(String, u64)> {
    array(json, key)
        .iter()
        .filter_map(|row| {
            let name = row.get("name").and_then(Value::as_str)?.to_string();
            let count = row.get("count").and_then(Value::as_u64)?;
            Some((name, count))
        })
        .collect()
}

fn join_inputs(json: &Value) -> String {
    match json.get("inputs") {
        Some(Value::Array(items)) => {
            let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if names.is_empty() {
                "(none)".to_string()
            } else {
                names.join(", ")
            }
        }
        Some(Value::Object(_)) => "(none)".to_string(),
        _ => "(none)".to_string(),
    }
}

fn as_estimate(text: &str) -> String {
    if text.starts_with("estimate:") {
        text.to_string()
    } else {
        format!("estimate: {text}")
    }
}

fn combine_note(estimate: Option<&str>, extra: Option<String>) -> Option<String> {
    match (estimate.filter(|text| !text.is_empty()), extra) {
        (Some(estimate), Some(extra)) => Some(format!("{estimate} {extra}")),
        (Some(estimate), None) => Some(estimate.to_string()),
        (None, Some(extra)) => Some(extra),
        (None, None) => None,
    }
}

struct Shell {
    period: String,
    scope_label: &'static str,
    scope: String,
    total: String,
    note: Option<String>,
}

struct Columns {
    top_unit: String,
    top_rows: Vec<Vec<String>>,
    finding_unit: String,
    findings: Vec<Vec<String>>,
    fix_lang: &'static str,
    fix_body: Option<String>,
}

fn page(tool: Tool, shell: Shell, columns: Columns) -> Teardown {
    Teardown {
        title: format!("{} teardown", tool.bin_name()),
        period: shell.period,
        scope_label: shell.scope_label.to_string(),
        scope: shell.scope,
        total: shell.total,
        note: shell.note,
        top_headers: top_headers(&columns.top_unit),
        top_rows: columns.top_rows,
        finding_headers: finding_headers(&columns.finding_unit),
        findings: columns.findings,
        fix_lang: columns.fix_lang.to_string(),
        fix_body: columns.fix_body,
    }
}

fn expect_schema(json: &Value, tool: &str, version: u64) -> Result<(), Fault> {
    match json.get("schema_version").and_then(Value::as_u64) {
        Some(got) if got == version => Ok(()),
        Some(_) => Err(Fault::new(format!(
            "{tool} JSON heeft een andere schema_version dan {version}"
        ))),
        None => Err(Fault::new(format!("{tool} JSON mist schema_version"))),
    }
}

fn expect_tool(json: &Value, name: &str) -> Result<(), Fault> {
    match json.get("tool").and_then(Value::as_str) {
        Some(got) if got == name => Ok(()),
        Some(got) => Err(Fault::new(format!(
            "dit is JSON van {got}, niet van {name}"
        ))),
        None => Err(Fault::new(format!("{name} JSON mist tool"))),
    }
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn shorten(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_becomes_a_drop_rule() {
        assert_eq!(
            mask_to_regex("user <NUM> failed at <TS> from <IP>"),
            r"user \d+ failed at \S+ from \S+"
        );
        assert_eq!(mask_to_regex("disk.full"), r"disk\.full");
    }

    #[test]
    fn log_report_has_the_shared_shape_and_a_drop_rule() {
        let json = serde_json::json!({
            "schema_version": 1,
            "tool": "log-bill",
            "estimate": "estimate: extrapolation of one sample with your own prices; not an official rate",
            "inputs": {"paths": ["app.log"]},
            "window": {"seconds": 60, "factor": "1440"},
            "totals": {"lines": 2, "bytes": 40},
            "patterns": [{
                "id": "Pabc",
                "text": "user <NUM> failed at <TS> from <IP>",
                "lines": 2,
                "bytes": 40,
                "share": "100.0",
                "rest": false
            }],
            "costs": {
                "currency": "USD",
                "totals": {"ingest": {"per_month": "0.30"}},
                "patterns": [{"id": "Pabc", "ingest": {"per_month": "0.30"}}]
            }
        });
        let text = crate::model::render(&log_report(&json).unwrap());
        assert!(text.starts_with("# log-bill teardown\n"), "{text}");
        assert!(
            text.contains("Period: sample (60s, factor 1440)\n"),
            "{text}"
        );
        assert!(text.contains("Inputs: app.log\n"), "{text}");
        assert!(text.contains("Total net: 0.30 USD\n"), "{text}");
        assert!(text.contains("## Top jobs\n"), "{text}");
        assert!(text.contains("## Findings\n"), "{text}");
        assert!(text.contains("| Amount (USD) |"), "{text}");
        assert!(text.contains("| Savings (USD) |"), "{text}");
        assert!(
            text.contains("drop user \\d+ failed at \\S+ from \\S+\n"),
            "{text}"
        );
    }

    fn many_patterns(count: usize) -> Value {
        let patterns: Vec<Value> = (0..count)
            .map(|n| {
                serde_json::json!({
                    "id": format!("P{n}"),
                    "text": format!("line {n}"),
                    "lines": 1,
                    "bytes": n as u64 + 1,
                    "share": "0.1",
                    "rest": false
                })
            })
            .collect();
        // Prices in the opposite order, so a lookup cannot rely on the position.
        let prices: Vec<Value> = (0..count)
            .rev()
            .map(|n| {
                serde_json::json!({
                    "id": format!("P{n}"),
                    "ingest": {"per_month": format!("{n}.00")}
                })
            })
            .collect();
        serde_json::json!({
            "schema_version": 1,
            "tool": "log-bill",
            "inputs": {"paths": ["app.log"]},
            "totals": {"lines": count, "bytes": count},
            "patterns": patterns,
            "costs": {
                "currency": "USD",
                "totals": {"ingest": {"per_month": "1.00"}},
                "patterns": prices
            }
        })
    }

    #[test]
    fn many_log_patterns_are_each_priced_from_their_own_id() {
        // B-2: the price lookup was one scan of `costs.patterns` per pattern.
        // 40 000 patterns took 30 s in release; with the index it is one pass.
        let count = 40_000;
        let text = crate::model::render(&log_report(&many_patterns(count)).unwrap());
        let last = count - 1;
        assert!(
            text.contains(&format!("| line {last} | P{last} | {last}.00 |")),
            "{text}"
        );
        let fifth = count - 5;
        assert!(
            text.contains(&format!("| line {fifth} | P{fifth} | {fifth}.00 |")),
            "{text}"
        );
    }

    #[test]
    fn the_first_price_for_an_id_still_wins() {
        let mut json = many_patterns(1);
        json["costs"]["patterns"] = serde_json::json!([
            {"id": "P0", "ingest": {"per_month": "1.00"}},
            {"id": "P0", "ingest": {"per_month": "999.00"}}
        ]);
        let text = crate::model::render(&log_report(&json).unwrap());
        assert!(text.contains("| line 0 | P0 | 1.00 |"), "{text}");
        assert!(!text.contains("999.00"), "{text}");
    }

    #[test]
    fn a_pattern_without_a_price_shows_its_bytes() {
        let mut json = many_patterns(1);
        json["costs"]["patterns"] = serde_json::json!([{"id": "other"}]);
        let text = crate::model::render(&log_report(&json).unwrap());
        assert!(text.contains("| line 0 | P0 | 1 |"), "{text}");
    }

    #[test]
    fn many_dd_keys_keep_their_order_and_counts() {
        // B-2, dd part: one lookup per key scanned every key so far.
        let count = 60_000;
        let keys: Vec<Value> = (0..count)
            .map(|n| serde_json::json!({"key": format!("k{n}"), "added": 1, "values": 2}))
            .collect();
        let unbounded: Vec<Value> = (0..count)
            .step_by(2)
            .map(|n| serde_json::json!({"key": format!("k{n}"), "reasons": ["uuid-like"]}))
            .collect();
        let json = serde_json::json!({
            "tags": [
                {"name": "m", "keys": keys},
                {"name": "m", "keys": [{"key": "k1", "added": 2}]}
            ],
            "unbounded": [{"name": "m", "keys": unbounded}]
        });
        let tags = collect_tags(&json);
        assert_eq!(tags.len(), count);
        assert_eq!(tags[0].key, "k0");
        assert_eq!(tags[count - 1].key, format!("k{}", count - 1));
        assert_eq!(tags[1].added, 3);
        assert!(tags[0].unbounded && !tags[1].unbounded);
        let dropped = tags_to_drop(&tags);
        assert_eq!(dropped.len(), count / 2);
        let allow = suggest_allow(&tags, &dropped).unwrap();
        assert!(allow.starts_with("k1,k3,k5,"), "{allow}");
        assert_eq!(allow.split(',').count(), count / 2);
    }

    #[test]
    fn dd_report_suggests_an_allowlist() {
        let json = serde_json::json!({
            "schema_version": 1,
            "tool": "dd-metric-bill",
            "estimate": "estimate: one snapshot",
            "inputs": ["sample.dogstatsd"],
            "total": 4,
            "metrics": [{"name": "request.Latency", "type": "count", "count": 4}],
            "tags": [{
                "name": "request.Latency",
                "type": "count",
                "keys": [
                    {"key": "endpoint", "added": 1, "values": 2},
                    {"key": "host", "added": 1, "values": 2},
                    {"key": "status", "added": 1, "values": 2}
                ]
            }],
            "unbounded": []
        });
        let text = crate::model::render(&dd(&json).unwrap());
        assert!(text.starts_with("# dd-metric-bill teardown\n"), "{text}");
        assert!(text.contains("Total net: 4 custom metrics\n"), "{text}");
        assert!(text.contains("| request.Latency | count | 4 |"), "{text}");
        assert!(text.contains("tag endpoint adds 1 series"), "{text}");
        assert!(text.contains("--allow-tags host,status\n"), "{text}");
        assert!(text.contains("| Savings (series) |"), "{text}");
    }

    #[test]
    fn rum_report_prices_the_largest_type() {
        let json = serde_json::json!({
            "schema_version": 1,
            "tool": "rum-meter",
            "estimate": "HCP Terraform bills the hourly peak; this is a single snapshot",
            "inputs": ["state.json"],
            "total_rum": 2,
            "by_module": [{"name": "module.child", "count": 2}],
            "by_type": [{"name": "random_id", "count": 2}],
            "costs": [{"name": "alpha", "currency": "EUR", "amount": "10.00"}]
        });
        let text = crate::model::render(&rum(&json).unwrap());
        assert!(text.starts_with("# rum-meter teardown\n"), "{text}");
        assert!(text.contains("State: state.json\n"), "{text}");
        assert!(text.contains("Total net: 10.00 EUR (alpha)\n"), "{text}");
        assert!(text.contains("| module.child | module | 10.00 |"), "{text}");
        assert!(
            text.contains("| T1 | random_id | 2 of 2 RUM, tier alpha | 10.00 |"),
            "{text}"
        );
        assert!(text.contains("Largest billed type: random_id\n"), "{text}");
        assert!(
            text.contains("estimate: HCP Terraform bills the hourly peak"),
            "{text}"
        );
    }

    #[test]
    fn a_key_that_is_unbounded_in_another_metric_names_that_metric() {
        let json = serde_json::json!({
            "schema_version": 1,
            "tool": "dd-metric-bill",
            "inputs": ["sample.dogstatsd"],
            "total": 9,
            "metrics": [
                {"name": "alpha", "type": "count", "count": 5},
                {"name": "beta", "type": "count", "count": 4}
            ],
            "tags": [
                {"name": "alpha", "type": "count", "keys": [{"key": "user", "added": 5, "values": 5}]},
                {"name": "beta", "type": "count", "keys": [
                    {"key": "user", "added": 4, "values": 4},
                    {"key": "region", "added": 1, "values": 2}
                ]}
            ],
            "unbounded": [{"name": "beta", "keys": [{"key": "user", "reasons": ["uuid-like"]}]}]
        });
        let tags = collect_tags(&json);
        assert_eq!(tags.len(), 3);
        let alpha = tags.iter().find(|t| t.metric == "alpha").unwrap();
        assert!(!alpha.unbounded);
        assert_eq!(alpha.added, 5);
        let dropped = tags_to_drop(&tags);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].metric, "beta");
        assert_eq!(dropped[0].added, 4);
        // The allowlist works on key names: `user` leaves it everywhere.
        assert_eq!(suggest_allow(&tags, &dropped).as_deref(), Some("region"));
        let text = crate::model::render(&dd(&json).unwrap());
        assert!(text.contains("| beta user |"), "{text}");
        assert!(text.contains("adds 4 series"), "{text}");
        assert!(!text.contains("adds 9 series"), "{text}");
    }
}
