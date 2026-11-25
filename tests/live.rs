use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cost_check::{produce, Bins, Job, Tool, ToolChoice};

/// Runs the four real tools. It is ignored in a plain `cargo test`, because
/// a machine without the tools proves nothing, and a skipped test must not
/// count as a pass. Run it on purpose:
///
/// ```sh
/// COST_CHECK_CI_BILL=... COST_CHECK_LOG_BILL=... COST_CHECK_DD_METRIC_BILL=... \
/// COST_CHECK_RUM_METER=... COST_CHECK_CI_BILL_SAMPLES=<ci-bill samples dir> \
///   cargo test --test live -- --ignored
/// ```
///
/// With `--ignored` a missing tool or sample directory fails the test.
#[test]
#[ignore = "needs the four tool binaries and the ci-bill samples; see the doc comment"]
fn live_reports() {
    let bins = Bins::discover(&cost_check::Overrides::default());
    if let Err(err) = bins.require_all() {
        panic!("live_reports: {err}. Zet COST_CHECK_CI_BILL en de andere drie.");
    }
    let scratch = Scratch::new();

    same_as_ci_bill(&bins);
    log_page(&bins, &scratch);
    dd_page(&bins, &scratch);
    rum_page(&bins, &scratch);
    upload_is_removed(&scratch);
}

fn same_as_ci_bill(bins: &Bins) {
    let samples = PathBuf::from(
        std::env::var_os("COST_CHECK_CI_BILL_SAMPLES")
            .expect("live_reports: zet COST_CHECK_CI_BILL_SAMPLES op de samples-map van ci-bill"),
    );
    let usage = samples.join("usage-sample.csv");
    let workflows = samples.join(".github/workflows");
    let bin = bins.get(Tool::Ci).expect("ci-bill");
    let out = Command::new(bin)
        .arg("report")
        .arg("--usage")
        .arg(&usage)
        .arg("--workflows")
        .arg(&workflows)
        .arg("--md")
        .output()
        .expect("ci-bill start");
    assert!(
        out.status.success(),
        "ci-bill: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = String::from_utf8(out.stdout).expect("md is tekst");
    let mut job = Job::new(ToolChoice::Fixed(Tool::Ci), usage, bins.clone());
    job.workflows = Some(workflows);
    let got = produce(&job).unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(got, expected);
}

fn log_page(bins: &Bins, scratch: &Scratch) {
    let input = scratch.write(
        "app.log",
        b"ERROR disk full on the checkout host\nERROR disk full on the checkout host\n",
    );
    let prices = scratch.write(
        "log-prices.json",
        br#"{"currency":"USD","source":"manual","date":"2026-10-24","ingest_per_gb":2}"#,
    );
    let mut job = Job::new(ToolChoice::Fixed(Tool::Log), input, bins.clone());
    job.window = Some("1m".to_string());
    job.prices = Some(prices);
    let text = produce(&job).unwrap_or_else(|err| panic!("{err}"));
    assert_page(&text, "log-bill");
    assert!(text.contains("Total net:"), "{text}");
    assert!(text.contains("USD"), "{text}");
    assert!(text.contains("## Suggested fix"), "{text}");
    assert!(text.contains("drop "), "{text}");
}

fn dd_page(bins: &Bins, scratch: &Scratch) {
    let input = scratch.write(
        "sample.dogstatsd",
        b"checkout.count:1|c|#host:a,endpoint:pay\n",
    );
    let job = Job::new(ToolChoice::Fixed(Tool::Dd), input, bins.clone());
    let text = produce(&job).unwrap_or_else(|err| panic!("{err}"));
    assert_page(&text, "dd-metric-bill");
    assert!(text.contains("custom metrics"), "{text}");
}

fn rum_page(bins: &Bins, scratch: &Scratch) {
    let input = scratch.write(
        "one.tfstate",
        br#"{
  "version": 4,
  "terraform_version": "1.9.0",
  "serial": 1,
  "lineage": "cost-check-live",
  "outputs": {},
  "resources": [
    {
      "mode": "managed",
      "type": "random_id",
      "name": "one",
      "provider": "provider[\"registry.terraform.io/hashicorp/random\"]",
      "instances": [{ "schema_version": 0, "attributes": { "id": "abc" } }]
    }
  ]
}
"#,
    );
    let prices = scratch.write(
        "rum-prices.json",
        br#"{"currency":"EUR","tiers":[{"name":"alpha","price_per_rum_hour":1}]}"#,
    );
    let mut job = Job::new(ToolChoice::Fixed(Tool::Rum), input, bins.clone());
    job.prices = Some(prices);
    let text = produce(&job).unwrap_or_else(|err| panic!("{err}"));
    assert_page(&text, "rum-meter");
    assert!(text.contains("EUR"), "{text}");
    assert!(text.contains("## Suggested fix"), "{text}");
}

fn upload_is_removed(scratch: &Scratch) {
    let upload = scratch.path().join("upload");
    fs::create_dir(&upload).expect("upload-map");
    let bin = env!("CARGO_BIN_EXE_cost-check");
    let mut child = Command::new(bin)
        .args(["serve", "--bind", "127.0.0.1:0"])
        .env("TMPDIR", &upload)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cost-check serve");
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let _err = drain(stderr);
    let guard = ChildGuard(child);
    let url = read_line(stdout, Duration::from_secs(15));
    let port = url
        .trim()
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse::<u16>()
        .unwrap_or_else(|_| panic!("geen adres: {url:?}"));
    let log = "ERROR disk full on the checkout host\n";
    let body = format!(
        "--cc\r\nContent-Disposition: form-data; name=\"tool\"\r\n\r\nlog-bill\r\n--cc\r\nContent-Disposition: form-data; name=\"export\"; filename=\"app.log\"\r\n\r\n{log}\r\n--cc--\r\n"
    );
    let request = format!(
        "POST /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary=cc\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).expect("verbind");
    client
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    client.write_all(request.as_bytes()).expect("schrijf");
    let mut buf = Vec::new();
    client.read_to_end(&mut buf).expect("lees");
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "{response}");
    let report = response.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(report.contains("# log-bill teardown"), "{report}");
    assert!(!report.contains("cost-check-"), "{report}");
    let left: Vec<_> = fs::read_dir(&upload)
        .expect("upload")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert!(left.is_empty(), "upload bleef staan: {left:?}");
    drop(guard);
}

fn assert_page(text: &str, tool: &str) {
    assert!(text.starts_with(&format!("# {tool} teardown\n")), "{text}");
    assert!(text.contains("\nPeriod: "), "{text}");
    assert!(text.contains("\nTotal net: "), "{text}");
    assert!(!text.contains("VOORBEELDDATA"), "{text}");
}

fn read_line(mut reader: impl Read + Send + 'static, timeout: Duration) -> String {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match reader.read(&mut byte) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
            }
        }
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(timeout).unwrap_or_else(|_| Vec::new());
    String::from_utf8_lossy(&line).into_owned()
}

fn drain(mut reader: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    })
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("cc-live-{}-{nanos}", std::process::id()));
        fs::create_dir(&path).expect("werkmap");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("schrijf");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
