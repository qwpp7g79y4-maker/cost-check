//! The one Markdown page every Cost Check uses.
//!
//! Headings and columns follow `ci-bill report --md`. A section with no rows
//! is left out, and so is a suggested fix with no body.

use std::fmt::Write as _;

pub struct Teardown {
    pub title: String,
    pub period: String,
    pub scope_label: String,
    pub scope: String,
    pub total: String,
    pub note: Option<String>,
    pub top_headers: Vec<String>,
    pub top_rows: Vec<Vec<String>>,
    pub finding_headers: Vec<String>,
    pub findings: Vec<Vec<String>>,
    pub fix_lang: String,
    pub fix_body: Option<String>,
}

pub fn render(page: &Teardown) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}", escape_display(&page.title));
    let _ = writeln!(out);
    let _ = writeln!(out, "Period: {}", escape_text(&page.period));
    let _ = writeln!(
        out,
        "{}: {}",
        escape_text(&page.scope_label),
        escape_text(&page.scope)
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "Total net: {}", escape_text(&page.total));
    if let Some(note) = page.note.as_deref().filter(|text| !text.is_empty()) {
        let _ = writeln!(out, "{}", escape_text(note));
    }
    write_table(&mut out, "Top jobs", &page.top_headers, &page.top_rows);
    write_table(&mut out, "Findings", &page.finding_headers, &page.findings);
    if let Some(body) = page
        .fix_body
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Suggested fix");
        let _ = writeln!(out);
        let body = escape_code(body);
        let fence = "`".repeat(fence_len(&body));
        let lang: String = page
            .fix_lang
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'))
            .collect();
        let _ = writeln!(out, "{fence}{lang}");
        out.push_str(&body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
        let _ = writeln!(out, "{fence}");
    }
    out
}

fn write_table(out: &mut String, heading: &str, headers: &[String], rows: &[Vec<String>]) {
    if rows.is_empty() || headers.is_empty() {
        return;
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## {heading}");
    let _ = writeln!(out);
    write_cells(out, headers);
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows {
        let mut cells = Vec::with_capacity(headers.len());
        for index in 0..headers.len() {
            cells.push(row.get(index).map(String::as_str).unwrap_or("").to_string());
        }
        write_cells(out, &cells);
    }
}

fn write_cells(out: &mut String, cells: &[String]) {
    out.push('|');
    for cell in cells {
        out.push(' ');
        out.push_str(&escape_cell(cell));
        out.push_str(" |");
    }
    out.push('\n');
}

/// Text for a table cell: everything `escape_text` does, and `|` too.
pub fn escape_cell(text: &str) -> String {
    escape_text(text).replace('|', "\\|")
}

/// Text that sits in a line or a cell of the report. A field from an export
/// cannot end the line, move the cursor, reverse the text, or turn into
/// Markdown: control and format characters become visible escapes, and the
/// characters that start a link, an image, HTML or code get a backslash.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '`' | '<' | '>' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            c => push_visible(&mut out, c, false),
        }
    }
    out
}

/// Control characters and bidi overrides become visible escapes, so one
/// field from an export cannot break the line or reverse the rest of it.
pub fn escape_display(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        push_visible(&mut out, ch, false);
    }
    out
}

/// A body for a code block. Newline and tab stay; every other control or
/// format character is escaped, like in a cell.
fn escape_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        push_visible(&mut out, ch, true);
    }
    out
}

/// Backticks in a fence: one more than the longest run in the body, at least 3.
fn fence_len(body: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for ch in body.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (longest + 1).max(3)
}

fn push_visible(out: &mut String, ch: char, keep_layout: bool) {
    match ch {
        '\n' if keep_layout => out.push('\n'),
        '\t' if keep_layout => out.push('\t'),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if (c as u32) < 0x20 || c as u32 == 0x7f => {
            let _ = write!(out, "\\x{:02x}", c as u32);
        }
        c if is_invisible(c) => {
            let _ = write!(out, "\\u{{{:04x}}}", c as u32);
        }
        c => out.push(c),
    }
}

/// C1 controls (U+0080..U+009F, which include CSI), format characters (Cf:
/// soft hyphen, Arabic letter mark, zero-width and bidi marks and overrides,
/// word joiner and invisible operators, bidi isolates, the byte order mark,
/// interlinear annotation, tag characters) and the line and paragraph
/// separators.
fn is_invisible(c: char) -> bool {
    matches!(
        c as u32,
        0x80..=0x9f
            | 0xad
            | 0x61c
            | 0x180e
            | 0x200b..=0x200f
            | 0x2028..=0x202e
            | 0x2060..=0x206f
            | 0xfeff
            | 0xfff9..=0xfffb
            | 0xe0000..=0xe007f
    )
}

pub fn top_headers(unit: &str) -> Vec<String> {
    vec![
        "Job".to_string(),
        "File:Line".to_string(),
        format!("Amount ({unit})"),
    ]
}

pub fn finding_headers(unit: &str) -> Vec<String> {
    vec![
        "Finding".to_string(),
        "File:Line".to_string(),
        "Message".to_string(),
        format!("Savings ({unit})"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare() -> Teardown {
        Teardown {
            title: "ci-bill teardown".to_string(),
            period: "2026-09-01 .. 2026-09-30".to_string(),
            scope_label: "Repositories".to_string(),
            scope: "example-org/sample-app".to_string(),
            total: "284.21 USD".to_string(),
            note: None,
            top_headers: top_headers("USD"),
            top_rows: Vec::new(),
            finding_headers: finding_headers("USD"),
            findings: Vec::new(),
            fix_lang: "diff".to_string(),
            fix_body: None,
        }
    }

    #[test]
    fn empty_sections_stay_out() {
        let text = render(&bare());
        assert_eq!(
            text,
            "# ci-bill teardown\n\nPeriod: 2026-09-01 .. 2026-09-30\nRepositories: example-org/sample-app\n\nTotal net: 284.21 USD\n"
        );
        assert!(!text.contains("## Top jobs"));
        assert!(!text.contains("## Findings"));
        assert!(!text.contains("## Suggested fix"));
    }

    #[test]
    fn a_pipe_in_a_cell_is_escaped() {
        let mut page = bare();
        page.top_rows.push(vec![
            "a|b".to_string(),
            "file:1".to_string(),
            "1.00".to_string(),
        ]);
        let text = render(&page);
        assert!(text.contains("| a\\|b | file:1 | 1.00 |"), "{text}");
    }

    #[test]
    fn every_control_and_format_character_is_visible() {
        for (ch, shown) in [
            ('\u{1b}', "\\x1b"),
            ('\u{7}', "\\x07"),
            ('\u{7f}', "\\x7f"),
            ('\r', "\\r"),
            ('\n', "\\n"),
            ('\u{9b}', "\\u{009b}"),
            ('\u{80}', "\\u{0080}"),
            ('\u{9f}', "\\u{009f}"),
            ('\u{200e}', "\\u{200e}"),
            ('\u{200f}', "\\u{200f}"),
            ('\u{2028}', "\\u{2028}"),
            ('\u{2029}', "\\u{2029}"),
            ('\u{202a}', "\\u{202a}"),
            ('\u{202e}', "\\u{202e}"),
            ('\u{2066}', "\\u{2066}"),
            ('\u{2069}', "\\u{2069}"),
            ('\u{feff}', "\\u{feff}"),
        ] {
            assert_eq!(escape_display(&ch.to_string()), shown, "{:x}", ch as u32);
            assert_eq!(escape_text(&ch.to_string()), shown, "{:x}", ch as u32);
        }
        assert_eq!(escape_display("é ü 日本 ß"), "é ü 日本 ß");
    }

    #[test]
    fn markdown_that_starts_a_link_html_or_code_gets_a_backslash() {
        assert_eq!(
            escape_text("<b>[a](x)</b> `c` \\"),
            "\\<b\\>\\[a\\](x)\\</b\\> \\`c\\` \\\\"
        );
        assert_eq!(escape_cell("a|b"), "a\\|b");
        assert_eq!(escape_text("284.21 USD (a-b_c)"), "284.21 USD (a-b_c)");
    }

    #[test]
    fn the_fence_is_longer_than_any_run_of_backticks_in_the_body() {
        assert_eq!(fence_len("no ticks"), 3);
        assert_eq!(fence_len("`a` ``b``"), 3);
        assert_eq!(fence_len("``` x"), 4);
        assert_eq!(fence_len("a ````` b ``"), 6);
        let mut page = bare();
        page.fix_body = Some(" ```\n ````\n## Injected\n".to_string());
        let text = render(&page);
        assert!(
            text.contains("\n`````diff\n ```\n ````\n## Injected\n`````\n"),
            "{text}"
        );
    }

    #[test]
    fn a_code_block_keeps_newlines_and_tabs_but_not_controls() {
        let mut page = bare();
        page.fix_body = Some("a\tb\n\u{1b}[2J\u{9b}\r\n".to_string());
        let text = render(&page);
        assert!(
            text.contains("```diff\na\tb\n\\x1b[2J\\u{009b}\\r\n```\n"),
            "{text}"
        );
    }
}
