#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use cost_check::{
    check_out_is_not_input, copy_stdout, escape_display, parse_bind, produce, read_limited,
    render_json, serve, write_out, Bins, Fault, Job, Overrides, SampleFormat, Tool, ToolChoice,
};

const FILE_CAP: usize = 32 * 1024 * 1024;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cost-check: {}", escape_display(err.message()));
            ExitCode::from(2)
        }
    }
}

#[derive(Parser)]
#[command(
    name = "cost-check",
    version,
    about = "Eén offline Cost Check-rapport uit een export."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Maak een rapport uit een export op schijf.
    Report(Box<ReportArgs>),
    /// Maak een rapport uit JSON die een tool al schreef.
    Render(RenderArgs),
    /// Open de lokale pagina. Alleen deze computer.
    Serve(ServeArgs),
}

#[derive(Args)]
struct ReportArgs {
    /// Export van de klant.
    #[arg(long, value_name = "BESTAND")]
    input: PathBuf,
    /// Tool: auto, ci-bill, log-bill, dd-metric-bill of rum-meter.
    #[arg(long, value_name = "TOOL", default_value = "auto")]
    tool: String,
    /// Map met workflow-YAML. Alleen voor ci-bill.
    #[arg(long, value_name = "MAP")]
    workflows: Option<PathBuf>,
    /// Prijsbestand van de tool.
    #[arg(long, value_name = "BESTAND")]
    prices: Option<PathBuf>,
    /// Venster van de log, bijvoorbeeld 1m.
    #[arg(long, value_name = "VENSTER")]
    window: Option<String>,
    /// Plan: pro of enterprise.
    #[arg(long, value_name = "PLAN")]
    plan: Option<String>,
    /// Aantal hosts. Alleen samen met --plan.
    #[arg(long, value_name = "N")]
    hosts: Option<u64>,
    /// Tag-sleutels die geïndexeerd mogen worden.
    #[arg(long, value_name = "LIJST")]
    allow_tags: Option<String>,
    /// Uren per maand. Alleen samen met --prices.
    #[arg(long, value_name = "N")]
    hours_per_month: Option<u64>,
    /// Invoerformaat: text, jsonl, prometheus of dogstatsd.
    #[arg(long, value_name = "FORMAAT")]
    format: Option<String>,
    /// Schrijf het rapport hierheen in plaats van naar stdout.
    #[arg(long, value_name = "BESTAND")]
    out: Option<PathBuf>,
    /// Wis de export nadat het rapport is geschreven.
    #[arg(long)]
    delete_input: bool,
    #[command(flatten)]
    bins: BinArgs,
}

#[derive(Args)]
struct RenderArgs {
    /// Tool: ci-bill, log-bill, dd-metric-bill of rum-meter.
    #[arg(long, value_name = "TOOL")]
    tool: String,
    /// JSON-uitvoer van die tool.
    #[arg(long, value_name = "BESTAND")]
    json: PathBuf,
    /// Unified diff. Alleen voor ci-bill.
    #[arg(long, value_name = "BESTAND")]
    fix: Option<PathBuf>,
}

#[derive(Args)]
struct ServeArgs {
    /// Adres. Alleen een loopback-adres.
    #[arg(long, value_name = "ADRES", default_value = "127.0.0.1:4517")]
    bind: String,
    #[command(flatten)]
    bins: BinArgs,
}

#[derive(Args, Clone, Default)]
struct BinArgs {
    /// Pad naar de ci-bill binary.
    #[arg(long, value_name = "PAD")]
    ci_bill: Option<PathBuf>,
    /// Pad naar de log-bill binary.
    #[arg(long, value_name = "PAD")]
    log_bill: Option<PathBuf>,
    /// Pad naar de dd-metric-bill binary.
    #[arg(long, value_name = "PAD")]
    dd_metric_bill: Option<PathBuf>,
    /// Pad naar de rum-meter binary.
    #[arg(long, value_name = "PAD")]
    rum_meter: Option<PathBuf>,
}

impl BinArgs {
    fn overrides(&self) -> Overrides {
        Overrides {
            ci_bill: self.ci_bill.clone(),
            log_bill: self.log_bill.clone(),
            dd_metric_bill: self.dd_metric_bill.clone(),
            rum_meter: self.rum_meter.clone(),
        }
    }
}

fn run(cli: Cli) -> Result<(), Fault> {
    match cli.command {
        Command::Report(args) => report(*args),
        Command::Render(args) => render(args),
        Command::Serve(args) => serve_page(args),
    }
}

fn report(args: ReportArgs) -> Result<(), Fault> {
    let choice = match args.tool.as_str() {
        "auto" => ToolChoice::Auto,
        other => ToolChoice::Fixed(Tool::parse(other).ok_or_else(|| Fault::new("onbekende tool"))?),
    };
    let format = match args.format.as_deref() {
        None => None,
        Some(text) => {
            Some(SampleFormat::parse(text).ok_or_else(|| Fault::new("onbekend formaat"))?)
        }
    };
    if let Some(out) = args.out.as_deref() {
        check_out_is_not_input(&args.input, out)?;
    }
    let bins = Bins::discover(&args.bins.overrides());
    let mut job = Job::new(choice, args.input.clone(), bins);
    job.workflows = args.workflows;
    job.prices = args.prices;
    job.window = args.window;
    job.plan = args.plan;
    job.hosts = args.hosts;
    job.allow_tags = args.allow_tags;
    job.hours_per_month = args.hours_per_month;
    job.format = format;
    let text = produce(&job)?;
    write_report(&text, args.out.as_deref())?;
    if args.delete_input && std::fs::remove_file(&args.input).is_err() {
        return Err(Fault::new(
            "het rapport is geschreven, maar de export kon niet worden gewist",
        ));
    }
    Ok(())
}

fn render(args: RenderArgs) -> Result<(), Fault> {
    let tool = Tool::parse(&args.tool).ok_or_else(|| Fault::new("onbekende tool"))?;
    let bytes = read_limited(&args.json, FILE_CAP)?;
    let fix = match args.fix.as_deref() {
        Some(path) => {
            let raw = read_limited(path, FILE_CAP)?;
            Some(String::from_utf8(raw).map_err(|_| Fault::new("de diff is geen tekst"))?)
        }
        None => None,
    };
    let text = render_json(tool, &bytes, fix.as_deref())?;
    copy_stdout(&text)
}

fn serve_page(args: ServeArgs) -> Result<(), Fault> {
    let addr = parse_bind(&args.bind)?;
    let bins = Bins::discover(&args.bins.overrides());
    serve(addr, bins)
}

fn write_report(text: &str, out: Option<&Path>) -> Result<(), Fault> {
    match out {
        Some(path) => write_out(path, text),
        None => copy_stdout(text),
    }
}
