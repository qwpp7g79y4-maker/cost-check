//! Which tool reads this export, from the first bytes only.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Ci,
    Log,
    Dd,
    Rum,
}

impl Tool {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "ci-bill" | "ci" => Some(Self::Ci),
            "log-bill" | "log" => Some(Self::Log),
            "dd-metric-bill" | "dd" => Some(Self::Dd),
            "rum-meter" | "rum" => Some(Self::Rum),
            _ => None,
        }
    }

    pub fn bin_name(self) -> &'static str {
        match self {
            Self::Ci => "ci-bill",
            Self::Log => "log-bill",
            Self::Dd => "dd-metric-bill",
            Self::Rum => "rum-meter",
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Ci, Self::Log, Self::Dd, Self::Rum]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    Text,
    Jsonl,
    Prometheus,
    Dogstatsd,
}

impl SampleFormat {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "text" => Some(Self::Text),
            "jsonl" => Some(Self::Jsonl),
            "prometheus" => Some(Self::Prometheus),
            "dogstatsd" => Some(Self::Dogstatsd),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Jsonl => "jsonl",
            Self::Prometheus => "prometheus",
            Self::Dogstatsd => "dogstatsd",
        }
    }
}

/// Look at the start of an export. The rest of the file is not required.
pub fn sniff_prefix(bytes: &[u8]) -> (Tool, Option<SampleFormat>) {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let end = bytes.len().min(8192);
    let text = String::from_utf8_lossy(&bytes[..end]);
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let header = first.to_ascii_lowercase();
    if header.contains("net_amount") && header.contains("repository") {
        return (Tool::Ci, None);
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let head = &trimmed[..char_boundary_at_most(trimmed, 8192)];
        if head.contains("\"resources\"") && head.contains("\"version\"") {
            return (Tool::Rum, None);
        }
        if first.contains("\"metric\"") {
            return (Tool::Dd, Some(SampleFormat::Jsonl));
        }
    }
    if is_dogstatsd(first) {
        return (Tool::Dd, Some(SampleFormat::Dogstatsd));
    }
    if first.starts_with("# HELP") || first.starts_with("# TYPE") || looks_like_prometheus(first) {
        return (Tool::Dd, Some(SampleFormat::Prometheus));
    }
    if first.trim_start().starts_with('{') {
        (Tool::Log, Some(SampleFormat::Jsonl))
    } else {
        (Tool::Log, Some(SampleFormat::Text))
    }
}

/// The largest index up to `max` that is not inside a character. The lossy
/// text can be longer than the bytes it came from, so a cut at a byte count
/// may fall inside U+FFFD.
fn char_boundary_at_most(text: &str, max: usize) -> usize {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn is_dogstatsd(line: &str) -> bool {
    let Some((name, rest)) = line.split_once(':') else {
        return false;
    };
    if name.is_empty() || name.contains(' ') {
        return false;
    }
    let Some((_, kind)) = rest.split_once('|') else {
        return false;
    };
    let kind = kind.split(['|', '#']).next().unwrap_or("");
    matches!(kind, "c" | "g" | "h" | "ms" | "d" | "s")
}

fn looks_like_prometheus(line: &str) -> bool {
    let Some(brace) = line.find('{') else {
        return false;
    };
    let name = &line[..brace];
    // An empty prefix is no metric name: `{"level":"error"}` is a JSON line.
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        && line.contains('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A JSONL line whose multibyte character starts `start` bytes in, so the
    /// 8192 byte cut falls inside it, plus a second line with more of them.
    fn jsonl_with_char_at(start: usize, ch: &str) -> Vec<u8> {
        let head = "{\"msg\":\"";
        let mut text = String::from(head);
        text.push_str(&"a".repeat(start - head.len()));
        text.push_str(ch);
        text.push_str("\"}\n{\"msg\":\"é 😀\"}\n");
        text.into_bytes()
    }

    #[test]
    fn a_cut_inside_a_multibyte_char_does_not_panic() {
        for ch in ["é", "日", "日日日", "😀", "😀😀"] {
            for start in 8184..=8193 {
                let bytes = jsonl_with_char_at(start, ch);
                let _ = sniff_prefix(&bytes);
                let mut with_bom = vec![0xef, 0xbb, 0xbf];
                with_bom.extend_from_slice(&bytes);
                let _ = sniff_prefix(&with_bom);
            }
        }
    }

    #[test]
    fn a_state_file_with_a_cut_char_is_still_found() {
        let mut text = String::from("{\"version\":4,\"resources\":[],\"x\":\"");
        text.push_str(&"a".repeat(8191 - text.len()));
        text.push_str("日日日\"}");
        assert_eq!(sniff_prefix(text.as_bytes()).0, Tool::Rum);
    }

    #[test]
    fn csv_usage_is_ci_bill() {
        let text = b"date,product,sku,net_amount,repository,workflow_path\n2026-09-01,actions,actions_linux,1,example/app,ci.yml\n";
        assert_eq!(sniff_prefix(text).0, Tool::Ci);
    }

    #[test]
    fn state_is_rum_meter() {
        let text = br#"{"version":4,"resources":[{"mode":"managed","type":"random_id","name":"one","instances":[{}]}]}"#;
        assert_eq!(sniff_prefix(text).0, Tool::Rum);
    }

    #[test]
    fn dogstatsd_is_dd() {
        let (tool, format) = sniff_prefix(b"checkout.count:1|c|#endpoint:pay\n");
        assert_eq!(tool, Tool::Dd);
        assert_eq!(format, Some(SampleFormat::Dogstatsd));
    }

    #[test]
    fn a_plain_log_stays_a_log() {
        let (tool, format) = sniff_prefix(b"2026-10-24T00:00:00Z ERROR disk full\n");
        assert_eq!(tool, Tool::Log);
        assert_eq!(format, Some(SampleFormat::Text));
    }

    #[test]
    fn jsonl_with_no_space_before_the_brace_is_a_log() {
        let (tool, format) =
            sniff_prefix(b"{\"level\":\"error\",\"msg\":\"disk full\",\"host\":\"a\"}\n");
        assert_eq!(tool, Tool::Log);
        assert_eq!(format, Some(SampleFormat::Jsonl));
    }

    #[test]
    fn a_prometheus_line_with_a_name_stays_prometheus() {
        let (tool, format) = sniff_prefix(b"up{job=\"api\"} 1\n");
        assert_eq!(tool, Tool::Dd);
        assert_eq!(format, Some(SampleFormat::Prometheus));
    }

    #[test]
    fn names_and_formats_round_trip() {
        assert_eq!(Tool::parse("log-bill"), Some(Tool::Log));
        assert_eq!(Tool::parse(" log "), Some(Tool::Log));
        assert_eq!(Tool::parse("dd-metric-bill"), Some(Tool::Dd));
        assert_eq!(Tool::parse("dd"), Some(Tool::Dd));
        assert_eq!(Tool::parse("rum-meter"), Some(Tool::Rum));
        assert_eq!(Tool::parse("rum"), Some(Tool::Rum));
        assert_eq!(Tool::parse("nope"), None);
        assert_eq!(SampleFormat::parse("text"), Some(SampleFormat::Text));
        assert_eq!(SampleFormat::parse("jsonl"), Some(SampleFormat::Jsonl));
        assert_eq!(
            SampleFormat::parse("prometheus"),
            Some(SampleFormat::Prometheus)
        );
        assert_eq!(
            SampleFormat::parse("dogstatsd"),
            Some(SampleFormat::Dogstatsd)
        );
        assert_eq!(SampleFormat::parse("nope"), None);
        assert_eq!(SampleFormat::Text.as_str(), "text");
        assert_eq!(SampleFormat::Jsonl.as_str(), "jsonl");
        assert_eq!(SampleFormat::Prometheus.as_str(), "prometheus");
        assert_eq!(SampleFormat::Dogstatsd.as_str(), "dogstatsd");
    }

    #[test]
    fn other_exports_pick_the_matching_tool() {
        let (tool, format) = sniff_prefix(br#"{"metric":"checkout.count","points":[]}"#);
        assert_eq!(tool, Tool::Dd);
        assert_eq!(format, Some(SampleFormat::Jsonl));

        let (tool, format) = sniff_prefix(b"# HELP requests A counter\nrequests_total 1\n");
        assert_eq!(tool, Tool::Dd);
        assert_eq!(format, Some(SampleFormat::Prometheus));

        let (tool, format) = sniff_prefix(b"http_requests_total{job=\"api\"} 1\n");
        assert_eq!(tool, Tool::Dd);
        assert_eq!(format, Some(SampleFormat::Prometheus));

        let (tool, format) = sniff_prefix(b" {\"msg\":\"disk full\"}\n");
        assert_eq!(tool, Tool::Log);
        assert_eq!(format, Some(SampleFormat::Jsonl));

        let (tool, format) = sniff_prefix(b":1|c\n");
        assert_eq!(tool, Tool::Log);
        assert_eq!(format, Some(SampleFormat::Text));
    }
}
