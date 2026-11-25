//! Run one of the four tools and render its JSON.
//!
//! The tools stay separate programs. This crate does not link them.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::adapt;
use crate::json;
use crate::model;
use crate::sniff::{self, SampleFormat, Tool};
use crate::Fault;

const STDOUT_CAP: usize = 32 * 1024 * 1024;
const STDERR_CAP: usize = 8 * 1024;
const TOOL_SECS: u64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolChoice {
    Auto,
    Fixed(Tool),
}

#[derive(Clone, Default)]
pub struct Overrides {
    pub ci_bill: Option<PathBuf>,
    pub log_bill: Option<PathBuf>,
    pub dd_metric_bill: Option<PathBuf>,
    pub rum_meter: Option<PathBuf>,
}

#[derive(Clone, Default)]
pub struct Bins {
    pub ci_bill: Option<PathBuf>,
    pub log_bill: Option<PathBuf>,
    pub dd_metric_bill: Option<PathBuf>,
    pub rum_meter: Option<PathBuf>,
}

impl Bins {
    pub fn discover(over: &Overrides) -> Self {
        Self {
            ci_bill: find("ci-bill", over.ci_bill.clone(), "COST_CHECK_CI_BILL"),
            log_bill: find("log-bill", over.log_bill.clone(), "COST_CHECK_LOG_BILL"),
            dd_metric_bill: find(
                "dd-metric-bill",
                over.dd_metric_bill.clone(),
                "COST_CHECK_DD_METRIC_BILL",
            ),
            rum_meter: find("rum-meter", over.rum_meter.clone(), "COST_CHECK_RUM_METER"),
        }
    }

    pub fn get(&self, tool: Tool) -> Result<&Path, Fault> {
        self.slot(tool).ok_or_else(|| {
            Fault::new(format!(
                "binary ontbreekt: {} (release-build of PATH)",
                tool.bin_name()
            ))
        })
    }

    pub fn require_all(&self) -> Result<(), Fault> {
        let missing: Vec<&str> = Tool::all()
            .into_iter()
            .filter(|tool| self.slot(*tool).is_none())
            .map(Tool::bin_name)
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Fault::new(format!(
                "binary ontbreekt: {}",
                missing.join(", ")
            )))
        }
    }

    fn slot(&self, tool: Tool) -> Option<&Path> {
        match tool {
            Tool::Ci => self.ci_bill.as_deref(),
            Tool::Log => self.log_bill.as_deref(),
            Tool::Dd => self.dd_metric_bill.as_deref(),
            Tool::Rum => self.rum_meter.as_deref(),
        }
    }
}

fn find(name: &str, over: Option<PathBuf>, env_key: &str) -> Option<PathBuf> {
    if let Some(path) = over {
        return Some(path);
    }
    if let Some(path) = std::env::var_os(env_key) {
        return Some(PathBuf::from(path));
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    find_in_dirs(name, &dirs, std::env::consts::EXE_SUFFIX)
}

/// The first `dir/name` or `dir/name<suffix>` that is a file. On Windows the
/// suffix is `.exe`, so `ci-bill.exe` is found as `ci-bill`.
fn find_in_dirs(name: &str, dirs: &[PathBuf], suffix: &str) -> Option<PathBuf> {
    for dir in dirs {
        let plain = dir.join(name);
        if plain.is_file() {
            return Some(plain);
        }
        if !suffix.is_empty() {
            let with_suffix = dir.join(format!("{name}{suffix}"));
            if with_suffix.is_file() {
                return Some(with_suffix);
            }
        }
    }
    None
}

pub struct Job {
    pub choice: ToolChoice,
    pub input: PathBuf,
    pub input_label: Option<String>,
    pub workflows: Option<PathBuf>,
    pub prices: Option<PathBuf>,
    pub window: Option<String>,
    pub plan: Option<String>,
    pub hosts: Option<u64>,
    pub allow_tags: Option<String>,
    pub hours_per_month: Option<u64>,
    pub format: Option<SampleFormat>,
    pub bins: Bins,
    pub redact: Vec<(String, String)>,
}

impl Job {
    pub fn new(choice: ToolChoice, input: PathBuf, bins: Bins) -> Self {
        Self {
            choice,
            input,
            input_label: None,
            workflows: None,
            prices: None,
            window: None,
            plan: None,
            hosts: None,
            allow_tags: None,
            hours_per_month: None,
            format: None,
            bins,
            redact: Vec::new(),
        }
    }
}

pub fn produce(job: &Job) -> Result<String, Fault> {
    match produce_inner(job) {
        Ok(text) => Ok(redact(text, &job.redact)),
        Err(err) => Err(Fault::new(redact(err.message().to_string(), &job.redact))),
    }
}

fn produce_inner(job: &Job) -> Result<String, Fault> {
    validate(job)?;
    let prefix = read_prefix(&job.input)?;
    let (sniffed, sniffed_format) = sniff::sniff_prefix(&prefix);
    let tool = match job.choice {
        ToolChoice::Auto => sniffed,
        ToolChoice::Fixed(tool) => tool,
    };
    let format = pick_format(job.choice, tool, job.format, sniffed_format)?;
    let bin = job.bins.get(tool)?.to_path_buf();
    if !bin.is_file() {
        return Err(Fault::new(format!("binary ontbreekt: {}", bin.display())));
    }
    let runs = commands(job, tool, format)?;
    let (json_bytes, _) = run_tool(&bin, &runs[0])?;
    let mut fix = None;
    if let Some(args) = runs.get(1) {
        if let Ok((stdout, _)) = run_tool(&bin, args) {
            if !stdout.iter().all(u8::is_ascii_whitespace) {
                fix = Some(String::from_utf8_lossy(&stdout).into_owned());
            }
        }
    }
    let json_bytes = strip_bom(&json_bytes);
    let value: Value = json::parse(json_bytes, "de tool gaf geen JSON terug")?;
    let page = adapt::teardown(tool, &value, fix.as_deref())?;
    Ok(model::render(&page))
}

fn validate(job: &Job) -> Result<(), Fault> {
    if let Some(window) = job.window.as_deref() {
        if !valid_window(window) {
            return Err(Fault::new(
                "venster moet een getal en s, m, h of d zijn, bijvoorbeeld 1m",
            ));
        }
    }
    if let Some(plan) = job.plan.as_deref() {
        if plan != "pro" && plan != "enterprise" {
            return Err(Fault::new("plan is pro of enterprise"));
        }
    }
    if job.hosts.is_some() && job.plan.is_none() {
        return Err(Fault::new("--hosts werkt alleen samen met --plan"));
    }
    if job.hours_per_month.is_some() && job.prices.is_none() {
        return Err(Fault::new(
            "--hours-per-month werkt alleen samen met --prices",
        ));
    }
    if let Some(tags) = job.allow_tags.as_deref() {
        if !valid_tags(tags) {
            return Err(Fault::new(
                "allow-tags is een lijst sleutels, gescheiden door komma's",
            ));
        }
    }
    if let Some(hours) = job.hours_per_month {
        if hours == 0 || hours > 1000 {
            return Err(Fault::new("uren per maand ligt tussen 1 en 1000"));
        }
    }
    Ok(())
}

fn valid_window(text: &str) -> bool {
    let Some(unit) = text.chars().last() else {
        return false;
    };
    if !matches!(unit, 's' | 'm' | 'h' | 'd') {
        return false;
    }
    let number = &text[..text.len() - unit.len_utf8()];
    !number.is_empty()
        && number.len() <= 9
        && number.chars().all(|c| c.is_ascii_digit())
        && !number.starts_with('0')
}

pub(crate) fn valid_tags(text: &str) -> bool {
    !text.is_empty()
        && text.split(',').all(|key| {
            !key.is_empty()
                && key.len() <= 64
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        })
}

/// The format handed to the tool.
///
/// An explicit `--format` must fit the tool, or the run is refused. The guess
/// from the first 8 KiB never overrules an explicit `--tool`: when it does
/// not fit that tool it is dropped and the tool reads the file its own way.
fn pick_format(
    choice: ToolChoice,
    tool: Tool,
    explicit: Option<SampleFormat>,
    sniffed: Option<SampleFormat>,
) -> Result<Option<SampleFormat>, Fault> {
    if let Some(format) = explicit {
        if !format_fits(tool, format) {
            return Err(Fault::new(format!(
                "formaat {} hoort niet bij {}",
                format.as_str(),
                tool.bin_name()
            )));
        }
        return Ok(Some(format));
    }
    Ok(match choice {
        ToolChoice::Auto => sniffed,
        ToolChoice::Fixed(_) => sniffed.filter(|format| format_fits(tool, *format)),
    })
}

fn format_fits(tool: Tool, format: SampleFormat) -> bool {
    match tool {
        Tool::Log => matches!(format, SampleFormat::Text | SampleFormat::Jsonl),
        Tool::Dd => matches!(
            format,
            SampleFormat::Dogstatsd | SampleFormat::Jsonl | SampleFormat::Prometheus
        ),
        Tool::Ci | Tool::Rum => false,
    }
}

/// Argument lists for the tool, as the operating system sees them. A path
/// stays the real path (no lossy text), and a path that starts with `-` is
/// never read as an option: `--name=PATH` for a flag, `--` before a positional.
fn commands(
    job: &Job,
    tool: Tool,
    format: Option<SampleFormat>,
) -> Result<Vec<Vec<OsString>>, Fault> {
    fn plain(text: &str) -> OsString {
        OsString::from(text)
    }
    fn flag_path(flag: &str, path: &Path) -> OsString {
        let mut arg = OsString::from(format!("--{flag}="));
        arg.push(path.as_os_str());
        arg
    }
    let input = job.input.as_os_str();
    match tool {
        Tool::Ci => {
            let mut base = vec![plain("report"), flag_path("usage", &job.input)];
            if let Some(dir) = &job.workflows {
                if !dir.is_dir() {
                    return Err(Fault::new("workflows is geen map"));
                }
                base.push(flag_path("workflows", dir));
            }
            let mut json = base.clone();
            json.push(plain("--json"));
            let mut runs = vec![json];
            if job.workflows.is_some() {
                let mut diff = base;
                diff.push(plain("--fix-diff"));
                runs.push(diff);
            }
            Ok(runs)
        }
        Tool::Log => {
            let mut args = vec![plain("scan"), plain("--json")];
            if let Some(window) = &job.window {
                args.push(plain("--window"));
                args.push(plain(window));
            }
            push_prices(&mut args, job)?;
            if let Some(format) = format {
                if format != SampleFormat::Text {
                    args.push(plain("--format"));
                    args.push(plain(format.as_str()));
                }
            }
            args.push(plain("--"));
            args.push(input.to_os_string());
            Ok(vec![args])
        }
        Tool::Dd => {
            let mut args = vec![plain("count"), plain("--json")];
            if let Some(plan) = &job.plan {
                args.push(plain("--plan"));
                args.push(plain(plan));
            }
            if let Some(hosts) = job.hosts {
                args.push(plain("--hosts"));
                args.push(plain(&hosts.to_string()));
            }
            if let Some(tags) = &job.allow_tags {
                args.push(plain("--allow-tags"));
                args.push(plain(tags));
            }
            push_prices(&mut args, job)?;
            if let Some(format) = format {
                if format != SampleFormat::Dogstatsd {
                    args.push(plain("--format"));
                    args.push(plain(format.as_str()));
                }
            }
            args.push(plain("--"));
            args.push(input.to_os_string());
            Ok(vec![args])
        }
        Tool::Rum => {
            let mut args = vec![plain("count"), plain("--json")];
            push_prices(&mut args, job)?;
            if let Some(hours) = job.hours_per_month {
                args.push(plain("--hours-per-month"));
                args.push(plain(&hours.to_string()));
            }
            args.push(plain("--"));
            args.push(input.to_os_string());
            Ok(vec![args])
        }
    }
}

fn push_prices(args: &mut Vec<OsString>, job: &Job) -> Result<(), Fault> {
    if let Some(prices) = &job.prices {
        if !prices.is_file() {
            return Err(Fault::new("prijsbestand ontbreekt"));
        }
        let mut arg = OsString::from("--prices=");
        arg.push(prices.as_os_str());
        args.push(arg);
    }
    Ok(())
}

fn run_tool(bin: &Path, args: &[OsString]) -> Result<(Vec<u8>, Vec<u8>), Fault> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| Fault::new(format!("kan {} niet starten: {err}", bin.display())))?;
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let mut stderr = child.stderr.take().expect("stderr pipe");
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&overflow);
    let out_thread = thread::spawn(move || read_capped(&mut stdout, STDOUT_CAP, &flag));
    let err_flag = Arc::new(AtomicBool::new(false));
    let err_thread = thread::spawn(move || read_capped(&mut stderr, STDERR_CAP, &err_flag));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None)
                if started.elapsed() > Duration::from_secs(TOOL_SECS)
                    || overflow.load(Ordering::Relaxed) =>
            {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                if overflow.load(Ordering::Relaxed) {
                    return Err(Fault::new("de tool schreef meer dan 32 MiB"));
                }
                return Err(Fault::new("de tool duurde langer dan 90 seconden"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(Fault::new(format!("wachten op de tool mislukte: {err}"))),
        }
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    if overflow.load(Ordering::Relaxed) {
        return Err(Fault::new("de tool schreef meer dan 32 MiB"));
    }
    if !status.success() {
        let code = status.code().unwrap_or(1);
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        let detail = if detail.chars().count() > 500 {
            let cut: String = detail.chars().take(500).collect();
            format!("{cut}…")
        } else {
            detail.to_string()
        };
        let detail = model::escape_display(&detail);
        if detail.is_empty() {
            return Err(Fault::new(format!(
                "{} stopte met code {code}",
                bin.file_name().and_then(|n| n.to_str()).unwrap_or("tool")
            )));
        }
        return Err(Fault::new(format!(
            "{} stopte met code {code}: {detail}",
            bin.file_name().and_then(|n| n.to_str()).unwrap_or("tool")
        )));
    }
    Ok((stdout, stderr))
}

fn read_capped(reader: &mut dyn Read, cap: usize, overflow: &AtomicBool) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut over = false;
    loop {
        match reader.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if over {
                    continue;
                }
                if buf.len().saturating_add(n) > cap {
                    over = true;
                    overflow.store(true, Ordering::Relaxed);
                    continue;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
        }
    }
    buf
}

/// Open a path the operator supplied, but only when it is a regular file.
///
/// `File::open` on a fifo blocks until some other process opens the write
/// end. On Linux, `O_NONBLOCK` makes that open return. `fstat` then rejects
/// every file type that is not a regular file. The flag does not change
/// later reads of a regular file.
fn open_regular(path: &Path) -> Result<fs::File, Fault> {
    let refused = || Fault::new(format!("kan {} niet openen", path.display()));
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux and Android: O_NONBLOCK is 0o4000.
        const O_NONBLOCK: i32 = 0o4000;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(path)
            .map_err(|_| refused())?;
        if !file.metadata().map_err(|_| refused())?.is_file() {
            return Err(refused());
        }
        Ok(file)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        // `metadata` does not block on a fifo. Refuse before `File::open`.
        if !fs::metadata(path).map_err(|_| refused())?.is_file() {
            return Err(refused());
        }
        fs::File::open(path).map_err(|_| refused())
    }
}

fn read_prefix(path: &Path) -> Result<Vec<u8>, Fault> {
    let mut file = open_regular(path)?;
    let mut buf = vec![0u8; 8192];
    let n = file
        .read(&mut buf)
        .map_err(|_| Fault::new(format!("kan {} niet lezen", path.display())))?;
    buf.truncate(n);
    if n == 0 {
        return Err(Fault::new("de export is leeg"));
    }
    Ok(buf)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
}

pub fn redact(mut text: String, rules: &[(String, String)]) -> String {
    let mut rules: Vec<&(String, String)> =
        rules.iter().filter(|rule| !rule.0.is_empty()).collect();
    rules.sort_by_key(|rule| std::cmp::Reverse(rule.0.len()));
    for (from, to) in rules {
        text = text.replace(from, to);
    }
    text
}

static DIR_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    pub fn create() -> Result<Self, Fault> {
        let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("cost-check-{}-{}-{}", std::process::id(), nanos, n));
        fs::create_dir(&path).map_err(|_| Fault::new("tijdelijke map kon niet worden gemaakt"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o700));
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write the report to `path`.
///
/// The text goes to a new file next to `path` that nobody else could have
/// made: it is created exclusively, so an existing file or a symlink under
/// that name is never followed, and a taken name is skipped. The new file then
/// replaces `path` in one rename. A `path` that is itself a symlink is refused.
pub fn write_out(path: &Path, text: &str) -> Result<(), Fault> {
    let refused = || Fault::new(format!("kan {} niet schrijven", path.display()));
    if fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Fault::new(format!(
            "{} is een symlink; kies een gewoon bestand",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (tmp, mut file) = create_exclusive(parent).ok_or_else(refused)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return Err(refused());
    }
    fs::rename(&tmp, path).map_err(|_| {
        let _ = fs::remove_file(&tmp);
        refused()
    })
}

fn create_exclusive(parent: &Path) -> Option<(PathBuf, fs::File)> {
    for _ in 0..32 {
        let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = parent.join(format!(
            ".cost-check-{}-{nanos:x}-{n}.tmp",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => return Some((tmp, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Refuse a report path that is the export itself: the write would replace
/// the export, and `--delete-input` would then remove the report too.
pub fn check_out_is_not_input(input: &Path, out: &Path) -> Result<(), Fault> {
    if same_file(input, out) {
        return Err(Fault::new(
            "--out is hetzelfde bestand als --input; kies een ander pad",
        ));
    }
    Ok(())
}

fn same_file(input: &Path, out: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(a), Ok(b)) = (fs::metadata(input), fs::metadata(out)) {
            if a.dev() == b.dev() && a.ino() == b.ino() {
                return true;
            }
        }
    }
    let Ok(input) = fs::canonicalize(input) else {
        return false;
    };
    let name = out.file_name();
    let dir = out
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    match (fs::canonicalize(dir), name) {
        (Ok(dir), Some(name)) => dir.join(name) == input,
        _ => false,
    }
}

pub fn read_limited(path: &Path, cap: usize) -> Result<Vec<u8>, Fault> {
    let file = open_regular(path)?;
    let mut limited = file.take(cap as u64 + 1);
    let mut buf = Vec::new();
    limited
        .read_to_end(&mut buf)
        .map_err(|_| Fault::new(format!("kan {} niet lezen", path.display())))?;
    if buf.len() > cap {
        return Err(Fault::new("het bestand is groter dan 32 MiB"));
    }
    Ok(buf)
}

/// Write to the real standard output and report every failure.
///
/// `println!` panics on a full disk, and Rust's own `Stdout` treats a closed
/// descriptor as success. This clones the descriptor first, so a closed
/// stdout is an error and a full one is an error too.
pub fn write_stdout(text: &str) -> Result<(), Fault> {
    let fail = || Fault::new("kan niet naar stdout schrijven");
    if stdout_was_closed() {
        return Err(fail());
    }
    io::stdout().flush().map_err(|_| fail())?;
    let mut out = stdout_file().map_err(|_| fail())?;
    out.write_all(text.as_bytes()).map_err(|_| fail())?;
    out.flush().map_err(|_| fail())
}

/// Rust's start-up code gives a closed stdout a fresh `/dev/null`, opened
/// read-write, so a write to it succeeds and the report is lost without an
/// error. A shell's `> /dev/null` opens it write-only. On Linux that
/// difference is visible, so the first case is refused. Elsewhere a closed
/// stdout cannot be told from `/dev/null`.
#[cfg(target_os = "linux")]
fn stdout_was_closed() -> bool {
    let Ok(target) = fs::read_link("/proc/self/fd/1") else {
        return false;
    };
    if target != Path::new("/dev/null") {
        return false;
    }
    let Ok(info) = fs::read_to_string("/proc/self/fdinfo/1") else {
        return false;
    };
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:"))
        .and_then(|value| u32::from_str_radix(value.trim(), 8).ok());
    // O_ACCMODE is 3 and O_RDWR is 2.
    matches!(flags, Some(flags) if flags & 3 == 2)
}

#[cfg(not(target_os = "linux"))]
fn stdout_was_closed() -> bool {
    false
}

#[cfg(unix)]
fn stdout_file() -> io::Result<fs::File> {
    use std::os::fd::AsFd;
    Ok(fs::File::from(io::stdout().as_fd().try_clone_to_owned()?))
}

#[cfg(windows)]
fn stdout_file() -> io::Result<fs::File> {
    use std::os::windows::io::AsHandle;
    Ok(fs::File::from(
        io::stdout().as_handle().try_clone_to_owned()?,
    ))
}

#[cfg(not(any(unix, windows)))]
fn stdout_file() -> io::Result<fs::File> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

pub fn copy_stdout(text: &str) -> Result<(), Fault> {
    write_stdout(text).map_err(|_| Fault::new("kan het rapport niet schrijven"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_dir_is_removed_on_drop() {
        let dir = WorkDir::create().unwrap();
        let path = dir.path().to_path_buf();
        fs::write(path.join("export.txt"), b"abc").unwrap();
        assert!(path.join("export.txt").is_file());
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn redact_replaces_the_longest_prefix_first() {
        let text = redact(
            "see /tmp/cost-check-1/workflows/lint.yml".to_string(),
            &[
                ("/tmp/cost-check-1/".to_string(), String::new()),
                ("/tmp/cost-check-1/workflows/".to_string(), String::new()),
            ],
        );
        assert_eq!(text, "see lint.yml");
    }

    #[test]
    fn read_limited_and_prefix_read_a_regular_file() {
        let dir = WorkDir::create().unwrap();
        let path = dir.path().join("export.txt");
        fs::write(&path, b"hello").unwrap();
        assert_eq!(read_limited(&path, 64).unwrap(), b"hello");
        assert_eq!(read_prefix(&path).unwrap(), b"hello");
    }

    #[cfg(unix)]
    #[test]
    fn fifo_read_fails_within_two_seconds() {
        use std::os::unix::fs::FileTypeExt;

        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let scratch = Scratch(
            std::env::temp_dir().join(format!("cost-check-fifo-{}-{nanos}", std::process::id())),
        );
        fs::create_dir(&scratch.0).unwrap();
        let fifo = scratch.0.join("export.fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo starten");
        assert!(made.success(), "mkfifo faalde");
        assert!(fs::metadata(&fifo).unwrap().file_type().is_fifo());

        let started = Instant::now();
        expect_fifo_error("read_limited", &fifo, |path| read_limited(path, 64));
        expect_fifo_error("read_prefix", &fifo, read_prefix);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "fifo-test duurde {:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    fn expect_fifo_error(label: &str, path: &Path, read: fn(&Path) -> Result<Vec<u8>, Fault>) {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        let owned = path.to_path_buf();
        let worker = thread::spawn(move || {
            let _ = tx.send(read(&owned));
        });
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Err(err)) => {
                assert!(
                    err.message().contains("niet openen"),
                    "{label}: {}",
                    err.message()
                );
                worker.join().expect(label);
            }
            Ok(Ok(bytes)) => panic!("{label}: fifo leverde {} bytes", bytes.len()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("{label}: hing langer dan 2 seconden op een fifo");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
                Ok(()) => panic!("{label}: geen antwoord"),
                Err(payload) => std::panic::resume_unwind(payload),
            },
        }
    }

    #[test]
    fn window_rejects_zero_and_words() {
        assert!(valid_window("1m"));
        assert!(valid_window("15m"));
        assert!(!valid_window("0m"));
        assert!(!valid_window("1"));
        assert!(!valid_window("1x"));
        assert!(!valid_window("m"));
    }

    #[test]
    fn tags_are_a_comma_list() {
        assert!(valid_tags("host"));
        assert!(valid_tags("host.endpoint_a:b-c"));
        assert!(!valid_tags(""));
        assert!(!valid_tags("bad tag"));
        assert!(!valid_tags("a,,b"));
        assert!(!valid_tags(&"h".repeat(65)));
    }

    #[test]
    fn produce_rejects_bad_flags_before_the_tool() {
        let mut job = Job::new(
            ToolChoice::Fixed(Tool::Log),
            PathBuf::from("missing"),
            Bins::default(),
        );
        job.window = Some("nope".to_string());
        assert!(produce(&job).unwrap_err().message().contains("venster"));

        job.window = None;
        job.plan = Some("free".to_string());
        assert!(produce(&job).unwrap_err().message().contains("plan"));

        job.plan = None;
        job.hosts = Some(2);
        assert!(produce(&job).unwrap_err().message().contains("--hosts"));

        job.hosts = None;
        job.hours_per_month = Some(10);
        assert!(produce(&job)
            .unwrap_err()
            .message()
            .contains("hours-per-month"));

        job.hours_per_month = None;
        job.allow_tags = Some("bad tag".to_string());
        assert!(produce(&job).unwrap_err().message().contains("allow-tags"));

        job.allow_tags = Some("host".to_string());
        job.hours_per_month = Some(0);
        job.prices = Some(PathBuf::from("prices.json"));
        assert!(produce(&job).unwrap_err().message().contains("uren"));

        job.hours_per_month = Some(1001);
        assert!(produce(&job).unwrap_err().message().contains("uren"));
    }

    #[test]
    fn an_explicit_tool_is_not_overruled_by_the_sniff() {
        let fixed = ToolChoice::Fixed(Tool::Rum);
        // A big Terraform state sniffs as jsonl; --tool rum-meter still runs.
        assert_eq!(
            pick_format(fixed, Tool::Rum, None, Some(SampleFormat::Jsonl)).unwrap(),
            None
        );
        // A guess that fits the chosen tool is kept; one that does not is dropped.
        let log = ToolChoice::Fixed(Tool::Log);
        assert_eq!(
            pick_format(log, Tool::Log, None, Some(SampleFormat::Jsonl)).unwrap(),
            Some(SampleFormat::Jsonl)
        );
        assert_eq!(
            pick_format(log, Tool::Log, None, Some(SampleFormat::Prometheus)).unwrap(),
            None
        );
        // Auto trusts the sniff, and an explicit format must fit.
        assert_eq!(
            pick_format(
                ToolChoice::Auto,
                Tool::Dd,
                None,
                Some(SampleFormat::Prometheus)
            )
            .unwrap(),
            Some(SampleFormat::Prometheus)
        );
        let err = pick_format(fixed, Tool::Rum, Some(SampleFormat::Jsonl), None).unwrap_err();
        assert!(err.message().contains("hoort niet bij"));
    }

    #[test]
    fn a_tool_is_found_with_and_without_an_exe_suffix() {
        let dir = WorkDir::create().unwrap();
        let empty = dir.path().join("empty");
        let bins = dir.path().join("bins");
        fs::create_dir(&empty).unwrap();
        fs::create_dir(&bins).unwrap();
        let dirs = vec![empty, bins.clone()];
        assert_eq!(find_in_dirs("ci-bill", &dirs, ".exe"), None);
        fs::write(bins.join("ci-bill.exe"), b"x").unwrap();
        assert_eq!(
            find_in_dirs("ci-bill", &dirs, ".exe"),
            Some(bins.join("ci-bill.exe"))
        );
        assert_eq!(find_in_dirs("ci-bill", &dirs, ""), None);
        fs::write(bins.join("ci-bill"), b"x").unwrap();
        assert_eq!(
            find_in_dirs("ci-bill", &dirs, ".exe"),
            Some(bins.join("ci-bill"))
        );
        // A directory with the right name is not a tool.
        fs::create_dir(bins.join("log-bill")).unwrap();
        assert_eq!(find_in_dirs("log-bill", &dirs, ".exe"), None);
    }

    #[test]
    fn a_path_keeps_its_bytes_and_a_dash_is_not_an_option() {
        let mut job = Job::new(
            ToolChoice::Fixed(Tool::Log),
            PathBuf::from("-x.log"),
            Bins::default(),
        );
        let runs = commands(&job, Tool::Log, None).unwrap();
        let args = &runs[0];
        assert_eq!(args[args.len() - 2], OsString::from("--"));
        assert_eq!(args[args.len() - 1], OsString::from("-x.log"));
        job.choice = ToolChoice::Fixed(Tool::Ci);
        let runs = commands(&job, Tool::Ci, None).unwrap();
        assert!(runs[0].contains(&OsString::from("--usage=-x.log")));
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_reaches_the_tool_unchanged() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let raw = std::ffi::OsStr::from_bytes(b"caf\xe9.log").to_os_string();
        let job = Job::new(
            ToolChoice::Fixed(Tool::Dd),
            PathBuf::from(raw.clone()),
            Bins::default(),
        );
        let runs = commands(&job, Tool::Dd, None).unwrap();
        assert_eq!(runs[0].last().unwrap().clone().into_vec(), raw.into_vec());
    }

    #[test]
    fn write_out_replaces_a_file_and_leaves_no_temp_file() {
        let dir = WorkDir::create().unwrap();
        let out = dir.path().join("report.md");
        write_out(&out, "one\n").unwrap();
        write_out(&out, "two\n").unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "two\n");
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, vec![OsString::from("report.md")]);
    }

    #[test]
    fn write_out_into_a_missing_folder_fails_cleanly() {
        let dir = WorkDir::create().unwrap();
        let err = write_out(&dir.path().join("nope").join("r.md"), "x").unwrap_err();
        assert!(err.message().contains("niet schrijven"));
    }

    #[cfg(unix)]
    #[test]
    fn write_out_refuses_a_symlink_and_never_follows_one() {
        let dir = WorkDir::create().unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"PRECIOUS").unwrap();
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        let err = write_out(&link, "report").unwrap_err();
        assert!(err.message().contains("symlink"), "{}", err.message());
        assert_eq!(fs::read(&victim).unwrap(), b"PRECIOUS");
        // A name taken by a symlink is skipped, not followed.
        let plain = dir.path().join("plain.md");
        write_out(&plain, "ok").unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"PRECIOUS");
        assert_eq!(fs::read_to_string(&plain).unwrap(), "ok");
    }

    #[test]
    fn out_equal_to_input_is_refused() {
        let dir = WorkDir::create().unwrap();
        let input = dir.path().join("export.log");
        fs::write(&input, b"data").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let same = dir.path().join("sub").join("..").join("export.log");
        assert!(check_out_is_not_input(&input, &input).is_err());
        assert!(check_out_is_not_input(&input, &same).is_err());
        assert!(check_out_is_not_input(&input, &dir.path().join("report.md")).is_ok());
        assert!(check_out_is_not_input(&input, Path::new("rapport-bestaat-niet.md")).is_ok());
        assert_eq!(fs::read(&input).unwrap(), b"data");
    }

    /// The permission test. It needs Unix modes, so it only runs on Unix.
    #[cfg(unix)]
    #[test]
    fn work_dir_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = WorkDir::create().unwrap();
        let mode = fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{mode:o}");
    }
}
