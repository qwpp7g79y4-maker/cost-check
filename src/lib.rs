#![forbid(unsafe_code)]

mod adapt;
mod json;
mod model;
mod money;
mod run;
mod serve;
mod sniff;

use std::fmt;

pub use model::escape_display;
pub use run::{
    check_out_is_not_input, copy_stdout, produce, read_limited, write_out, Bins, Job, Overrides,
    ToolChoice,
};
pub use serve::{check_bind, parse_bind, serve};
pub use sniff::{sniff_prefix, SampleFormat, Tool};

/// A failure the operator can read. It does not carry the export.
#[derive(Debug)]
pub struct Fault {
    message: String,
}

impl Fault {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Fault {}

/// Turn tool JSON into the shared Markdown report.
pub fn render_json(tool: Tool, bytes: &[u8], fix: Option<&str>) -> Result<String, Fault> {
    let value = json::parse(bytes, "de JSON is onleesbaar")?;
    let page = adapt::teardown(tool, &value, fix)?;
    Ok(model::render(&page))
}
