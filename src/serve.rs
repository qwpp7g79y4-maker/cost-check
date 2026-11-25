//! Loopback page. One upload becomes one report, then the file is removed.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use crate::run::{self, Bins, Job, ToolChoice, WorkDir};
use crate::sniff::{SampleFormat, Tool};
use crate::Fault;

const BODY_CAP: usize = 32 * 1024 * 1024;
const HEADER_CAP: usize = 16 * 1024;
const PAGE: &str = include_str!("page.html");

struct App {
    bins: Bins,
    port: u16,
}

pub fn check_bind(addr: SocketAddr) -> Result<(), Fault> {
    if addr.ip().is_loopback() {
        Ok(())
    } else {
        Err(Fault::new(
            "de pagina luistert alleen op een loopback-adres, bijvoorbeeld 127.0.0.1:4517",
        ))
    }
}

pub fn parse_bind(text: &str) -> Result<SocketAddr, Fault> {
    let text = text.trim();
    let owned;
    let text = if let Some(port) = text.strip_prefix("localhost:") {
        owned = format!("127.0.0.1:{port}");
        owned.as_str()
    } else {
        text
    };
    let addr: SocketAddr = text
        .parse()
        .map_err(|_| Fault::new("bind-adres is ongeldig"))?;
    check_bind(addr)?;
    Ok(addr)
}

pub fn serve(addr: SocketAddr, bins: Bins) -> Result<(), Fault> {
    check_bind(addr)?;
    bins.require_all()?;
    let listener = TcpListener::bind(addr).map_err(|_| {
        Fault::new(format!(
            "kan niet luisteren op {addr}. Kies een andere poort met --bind."
        ))
    })?;
    let port = listener
        .local_addr()
        .map_err(|_| Fault::new("geen poort"))?
        .port();
    run::write_stdout(&format!("http://127.0.0.1:{port}/\n"))?;
    let app = App { bins, port };
    for conn in listener.incoming() {
        match conn {
            Ok(sock) => {
                let _ = handle(sock, &app);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, app: &App) -> Result<(), Fault> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(err) => {
            let _ = write_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                None,
                err.message().as_bytes(),
            );
            return Err(err);
        }
    };
    if !host_ok(&request, app.port) {
        let _ = write_response(
            &mut stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            None,
            b"host is niet deze computer",
        );
        return Ok(());
    }
    if let Some(origin) = request.header("origin") {
        if !origin_ok(origin, app.port) {
            let _ = write_response(
                &mut stream,
                403,
                "Forbidden",
                "text/plain; charset=utf-8",
                None,
                b"alleen deze pagina mag een rapport vragen",
            );
            return Ok(());
        }
    }
    let response = route(&request, app);
    let _ = write_response(
        &mut stream,
        response.status,
        response.reason,
        response.content_type,
        response.disposition,
        &response.body,
    );
    Ok(())
}

struct Response {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    disposition: Option<String>,
    body: Vec<u8>,
}

fn route(request: &Request, app: &App) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Response {
            status: 200,
            reason: "OK",
            content_type: "text/html; charset=utf-8",
            disposition: None,
            body: PAGE.as_bytes().to_vec(),
        },
        ("POST", "/report") => match report(request, app) {
            Ok(body) => Response {
                status: 200,
                reason: "OK",
                content_type: "text/plain; charset=utf-8",
                disposition: Some("inline; filename=\"kostencheck.md\"".to_string()),
                body: body.into_bytes(),
            },
            Err(err) => Response {
                status: 400,
                reason: "Bad Request",
                content_type: "text/plain; charset=utf-8",
                disposition: None,
                body: err.message().as_bytes().to_vec(),
            },
        },
        ("GET", "/report") | ("POST", "/") => Response {
            status: 405,
            reason: "Method Not Allowed",
            content_type: "text/plain; charset=utf-8",
            disposition: None,
            body: b"niet zo".to_vec(),
        },
        _ => Response {
            status: 404,
            reason: "Not Found",
            content_type: "text/plain; charset=utf-8",
            disposition: None,
            body: b"niet gevonden".to_vec(),
        },
    }
}

fn report(request: &Request, app: &App) -> Result<String, Fault> {
    let form = parse_form(request.header("content-type").unwrap_or(""), &request.body)?;
    let export = form
        .export
        .as_ref()
        .filter(|(_, bytes)| !bytes.is_empty())
        .ok_or_else(|| Fault::new("kies een export"))?;
    let dir = WorkDir::create()?;
    let export_name = safe_name(&export.0, "export.txt");
    let export_path = unique_path(dir.path(), &export_name);
    let export_name = export_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.txt")
        .to_string();
    fs::write(&export_path, &export.1)
        .map_err(|_| Fault::new("de export kon niet worden gezet"))?;
    let mut redact = vec![(export_path.display().to_string(), export_name.clone())];
    let workflows = write_workflows(&dir, &form.workflows, &mut redact)?;
    let prices = write_prices(&dir, form.prices.as_ref(), &mut redact)?;
    let choice = match form.field("tool").unwrap_or("auto") {
        "" | "auto" => ToolChoice::Auto,
        other => ToolChoice::Fixed(Tool::parse(other).ok_or_else(|| Fault::new("onbekende tool"))?),
    };
    let format = match form.field("format").unwrap_or("") {
        "" => None,
        other => Some(SampleFormat::parse(other).ok_or_else(|| Fault::new("onbekend formaat"))?),
    };
    let mut job = Job::new(choice, export_path, app.bins.clone());
    job.workflows = workflows;
    job.prices = prices;
    job.window = blank(form.field("window"));
    job.plan = blank(form.field("plan"));
    job.hosts = parse_opt_u64(form.field("hosts"), "hosts")?;
    job.allow_tags = blank(form.field("allow_tags"));
    job.hours_per_month = parse_opt_u64(form.field("hours"), "uren")?;
    job.format = format;
    job.redact = redact;
    run::produce(&job)
}

fn write_workflows(
    dir: &WorkDir,
    files: &[(String, Vec<u8>)],
    redact: &mut Vec<(String, String)>,
) -> Result<Option<PathBuf>, Fault> {
    let files: Vec<&(String, Vec<u8>)> = files
        .iter()
        .filter(|(_, bytes)| !bytes.is_empty())
        .collect();
    if files.is_empty() {
        return Ok(None);
    }
    if files.len() > 40 {
        return Err(Fault::new("hoogstens 40 workflow-bestanden"));
    }
    let folder = dir.path().join("workflows");
    fs::create_dir(&folder).map_err(|_| Fault::new("workflow-map kon niet worden gemaakt"))?;
    for (name, bytes) in files {
        let name = safe_name(name, "workflow.yml");
        if !name.ends_with(".yml") && !name.ends_with(".yaml") {
            return Err(Fault::new("een workflow-bestand eindigt op .yml of .yaml"));
        }
        let path = unique_path(&folder, &name);
        fs::write(&path, bytes).map_err(|_| Fault::new("een workflow kon niet worden gezet"))?;
    }
    redact.push((format!("{}/", folder.display()), String::new()));
    Ok(Some(folder))
}

fn write_prices(
    dir: &WorkDir,
    prices: Option<&(String, Vec<u8>)>,
    redact: &mut Vec<(String, String)>,
) -> Result<Option<PathBuf>, Fault> {
    let Some((name, bytes)) = prices else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > 1024 * 1024 {
        return Err(Fault::new("het prijsbestand is groter dan 1 MiB"));
    }
    let name = safe_name(name, "prices.json");
    let path = unique_path(dir.path(), &name);
    fs::write(&path, bytes).map_err(|_| Fault::new("het prijsbestand kon niet worden gezet"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prices.json")
        .to_string();
    redact.push((path.display().to_string(), name));
    Ok(Some(path))
}

fn unique_path(folder: &std::path::Path, name: &str) -> PathBuf {
    let direct = folder.join(name);
    if !direct.exists() {
        return direct;
    }
    let (stem, ext) = name
        .rsplit_once('.')
        .map(|(stem, ext)| (stem, format!(".{ext}")))
        .unwrap_or((name, String::new()));
    for n in 2..100 {
        let candidate = folder.join(format!("{stem}-{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    folder.join(format!("{stem}-extra{ext}"))
}

fn safe_name(raw: &str, fallback: &str) -> String {
    let base = std::path::Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback);
    let mut out = String::new();
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.starts_with('.') || out == "." || out == ".." {
        out = fallback.to_string();
    }
    if out.len() > 80 {
        out.truncate(80);
    }
    out
}

fn blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn parse_opt_u64(value: Option<&str>, label: &str) -> Result<Option<u64>, Fault> {
    match blank(value) {
        None => Ok(None),
        Some(text) => text
            .parse::<u64>()
            .map(Some)
            .map_err(|_| Fault::new(format!("{label} is geen geheel getal"))),
    }
}

struct Form {
    fields: Vec<(String, String)>,
    export: Option<(String, Vec<u8>)>,
    prices: Option<(String, Vec<u8>)>,
    workflows: Vec<(String, Vec<u8>)>,
}

impl Form {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn parse_form(content_type: &str, body: &[u8]) -> Result<Form, Fault> {
    let boundary = boundary_of(content_type)?;
    let parts = split_multipart(body, boundary)?;
    let mut form = Form {
        fields: Vec::new(),
        export: None,
        prices: None,
        workflows: Vec::new(),
    };
    for part in parts {
        let name = part
            .name
            .filter(|name| !name.is_empty())
            .ok_or_else(|| Fault::new("een formulier-veld mist een naam"))?;
        if let Some(filename) = part.filename {
            if part.body.is_empty() {
                continue;
            }
            match name.as_str() {
                "export" => {
                    if form.export.is_some() {
                        return Err(Fault::new("één export"));
                    }
                    form.export = Some((filename, part.body));
                }
                "prices" => {
                    if form.prices.is_some() {
                        return Err(Fault::new("één prijsbestand"));
                    }
                    form.prices = Some((filename, part.body));
                }
                "workflow" => form.workflows.push((filename, part.body)),
                _ => {}
            }
        } else {
            let text =
                String::from_utf8(part.body).map_err(|_| Fault::new("een veld is geen tekst"))?;
            form.fields.push((name, text));
        }
    }
    Ok(form)
}

struct Part {
    name: Option<String>,
    filename: Option<String>,
    body: Vec<u8>,
}

fn boundary_of(content_type: &str) -> Result<&str, Fault> {
    let bytes = content_type.as_bytes();
    let needle = b"boundary=";
    let Some(index) = bytes
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
    else {
        return Err(Fault::new("multipart mist een boundary"));
    };
    let rest = content_type[index + needle.len()..].trim();
    let boundary = if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted
            .find('"')
            .ok_or_else(|| Fault::new("multipart mist een boundary"))?;
        &quoted[..end]
    } else {
        rest.split(';').next().unwrap_or(rest).trim()
    };
    if boundary.is_empty() || boundary.len() > 200 || boundary.as_bytes().contains(&b' ') {
        return Err(Fault::new("multipart mist een boundary"));
    }
    Ok(boundary)
}

fn split_multipart(body: &[u8], boundary: &str) -> Result<Vec<Part>, Fault> {
    let delim = format!("--{boundary}").into_bytes();
    let mut parts = Vec::new();
    let Some(start) = find_at(body, &delim, 0) else {
        return Err(Fault::new("multipart is onvolledig"));
    };
    let mut pos = start + delim.len();
    loop {
        if body.get(pos..pos + 2) == Some(b"--") {
            break;
        }
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        }
        let Some(next) = find_at(body, &delim, pos) else {
            return Err(Fault::new("multipart is onvolledig"));
        };
        let mut part = &body[pos..next];
        if part.ends_with(b"\r\n") {
            part = &part[..part.len() - 2];
        }
        parts.push(parse_part(part)?);
        if parts.len() > 80 {
            return Err(Fault::new("te veel formulier-velden"));
        }
        pos = next + delim.len();
    }
    Ok(parts)
}

fn parse_part(bytes: &[u8]) -> Result<Part, Fault> {
    let split =
        find_at(bytes, b"\r\n\r\n", 0).ok_or_else(|| Fault::new("multipart is onvolledig"))?;
    let header =
        std::str::from_utf8(&bytes[..split]).map_err(|_| Fault::new("multipart is onvolledig"))?;
    let body = bytes[split + 4..].to_vec();
    let mut name = None;
    let mut filename = None;
    for line in header.split("\r\n") {
        if !line
            .to_ascii_lowercase()
            .starts_with("content-disposition:")
        {
            continue;
        }
        name = disp_param(line, "name");
        filename = disp_param(line, "filename");
    }
    Ok(Part {
        name,
        filename,
        body,
    })
}

/// One parameter of a `Content-Disposition` header, by name, wherever it
/// stands in the list. `name` is not `filename`, and `filename*` is another
/// parameter. A value is a token or a quoted string with `\\` escapes.
fn disp_param(header: &str, key: &str) -> Option<String> {
    let rest = &header[header.find(':')? + 1..];
    let mut chars = rest.chars().peekable();
    let mut found = None;
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ';') {
            chars.next();
        }
        if chars.peek().is_none() {
            return found;
        }
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ';' {
                break;
            }
            name.push(c);
            chars.next();
        }
        if chars.next() != Some('=') {
            continue;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => value.extend(chars.next()),
                    c => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ';' {
                    break;
                }
                value.push(c);
                chars.next();
            }
            value = value.trim().to_string();
        }
        if found.is_none() && name.trim().eq_ignore_ascii_case(key) {
            found = Some(value);
        }
    }
}

fn find_at(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|index| index + from)
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Request, Fault> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end = loop {
        if buf.len() > HEADER_CAP {
            return Err(Fault::new("de aanvraag is te groot"));
        }
        let n = stream
            .read(&mut tmp)
            .map_err(|_| Fault::new("de aanvraag brak af"))?;
        if n == 0 {
            return Err(Fault::new("de aanvraag brak af"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(index) = find_at(&buf, b"\r\n\r\n", 0) {
            break index;
        }
    };
    let head = std::str::from_utf8(&buf[..header_end])
        .map_err(|_| Fault::new("de aanvraag is onleesbaar"))?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut pieces = request_line.split(' ');
    let method = pieces.next().unwrap_or("").to_string();
    let raw_path = pieces.next().unwrap_or("");
    let path = raw_path.split('?').next().unwrap_or("").to_string();
    if method.is_empty() || path.is_empty() {
        return Err(Fault::new("de aanvraag is onleesbaar"));
    }
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut body = buf[header_end + 4..].to_vec();
    if method == "POST" {
        let length: usize = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.parse().ok())
            .ok_or_else(|| Fault::new("content-length ontbreekt"))?;
        if length > BODY_CAP {
            return Err(Fault::new("de export is groter dan 32 MiB"));
        }
        while body.len() < length {
            let n = stream
                .read(&mut tmp)
                .map_err(|_| Fault::new("de aanvraag brak af"))?;
            if n == 0 {
                return Err(Fault::new("de aanvraag brak af"));
            }
            body.extend_from_slice(&tmp[..n]);
            if body.len() > BODY_CAP {
                return Err(Fault::new("de export is groter dan 32 MiB"));
            }
        }
        body.truncate(length);
    } else {
        body.clear();
    }
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn host_ok(request: &Request, port: u16) -> bool {
    let Some(host) = request.header("host") else {
        return false;
    };
    host == format!("127.0.0.1:{port}")
        || host == format!("localhost:{port}")
        || host == format!("[::1]:{port}")
}

fn origin_ok(origin: &str, port: u16) -> bool {
    origin == format!("http://127.0.0.1:{port}")
        || origin == format!("http://localhost:{port}")
        || origin == format!("http://[::1]:{port}")
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    disposition: Option<String>,
    body: &[u8],
) -> Result<(), Fault> {
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
        body.len()
    );
    if let Some(disposition) = disposition {
        head.push_str(&format!("Content-Disposition: {disposition}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|_| Fault::new("het antwoord kon niet worden geschreven"))?;
    stream
        .write_all(body)
        .map_err(|_| Fault::new("het antwoord kon niet worden geschreven"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn page_is_local_and_has_no_account() {
        let page = include_str!("page.html");
        assert!(page.contains("Geen account"));
        assert!(!page.contains("http://"));
        assert!(!page.contains("https://"));
        assert!(!page.to_ascii_lowercase().contains("cookie"));
    }

    #[test]
    fn a_price_file_cannot_replace_the_export_of_the_same_name() {
        let dir = WorkDir::create().unwrap();
        let export_path = dir.path().join("data.json");
        fs::write(&export_path, b"EXPORT").unwrap();
        let mut redact = Vec::new();
        let prices = (String::from("data.json"), b"PRICES".to_vec());
        let path = write_prices(&dir, Some(&prices), &mut redact)
            .unwrap()
            .unwrap();
        assert_ne!(path, export_path);
        assert_eq!(fs::read(&export_path).unwrap(), b"EXPORT");
        assert_eq!(fs::read(&path).unwrap(), b"PRICES");
    }

    #[test]
    fn public_bind_is_refused() {
        let addr: SocketAddr = "0.0.0.0:9".parse().unwrap();
        let err = check_bind(addr).unwrap_err();
        assert!(err.message().contains("loopback"));
    }

    #[test]
    fn get_returns_the_page() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let raw = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
        let response = exchange_on(listener, raw.as_bytes());
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.contains("Geen account"), "{response}");
    }

    #[test]
    fn another_origin_is_forbidden() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let raw = format!(
            "POST /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: https://evil.example\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"
        );
        let response = exchange_on(listener, raw.as_bytes());
        assert!(response.contains("403 Forbidden"), "{response}");
    }

    #[test]
    fn multipart_reads_export_and_tool() {
        let body = "\
--bound\r\n\
Content-Disposition: form-data; name=\"tool\"\r\n\
\r\n\
log-bill\r\n\
--bound\r\n\
Content-Disposition: form-data; name=\"export\"; filename=\"app.log\"\r\n\
\r\n\
hello\r\n\
--bound--\r\n";
        let form = parse_form("multipart/form-data; boundary=bound", body.as_bytes()).unwrap();
        assert_eq!(form.field("tool"), Some("log-bill"));
        assert_eq!(form.export.unwrap().1, b"hello");
    }

    #[test]
    fn multipart_name_does_not_depend_on_parameter_order() {
        let body = "\
--bound\r\n\
Content-Disposition: form-data; filename=\"app.log\"; name=\"export\"\r\n\
\r\n\
hello\r\n\
--bound--\r\n";
        let form = parse_form("multipart/form-data; boundary=bound", body.as_bytes()).unwrap();
        let (filename, bytes) = form.export.unwrap();
        assert_eq!(filename, "app.log");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn disposition_parameters_are_read_by_name() {
        let line = "Content-Disposition: form-data; filename=\"a.log\"; name=\"export\"";
        assert_eq!(disp_param(line, "name").as_deref(), Some("export"));
        assert_eq!(disp_param(line, "filename").as_deref(), Some("a.log"));
        let line = "content-disposition: form-data; Name=export; FILENAME=\"x\"";
        assert_eq!(disp_param(line, "name").as_deref(), Some("export"));
        assert_eq!(disp_param(line, "filename").as_deref(), Some("x"));
        // `filename*` is a different parameter, and a name inside a quoted
        // filename is not a parameter.
        let line = "Content-Disposition: form-data; filename*=UTF-8''a.log; filename=\"b; name=\\\"x\\\"\"; name=\"export\"";
        assert_eq!(disp_param(line, "name").as_deref(), Some("export"));
        assert_eq!(
            disp_param(line, "filename").as_deref(),
            Some("b; name=\"x\"")
        );
        let line = "Content-Disposition: form-data; filename=\"only.log\"";
        assert_eq!(disp_param(line, "name"), None);
    }

    #[cfg(unix)]
    #[test]
    fn post_report_renders_and_stops() {
        let scratch = ServeScratch::new();
        let stub = script(scratch.path(), "log-bill", LOG_JSON);
        let bins = Bins {
            log_bill: Some(stub),
            ..Bins::default()
        };
        let body = format!(
            "{tool}{window}{plan}{hosts}{tags}{hours}{format}{export}{prices}{flow_a}{flow_b}{flow_c}--bound--\r\n",
            tool = field("tool", "log-bill"),
            window = field("window", "1m"),
            plan = field("plan", "pro"),
            hosts = field("hosts", "2"),
            tags = field("allow_tags", "host.endpoint"),
            hours = field("hours", "10"),
            format = field("format", "jsonl"),
            export = file("export", "app log.txt", "ERROR disk full\n"),
            prices = file("prices", "prices.json", "{\"currency\":\"USD\"}\n"),
            flow_a = file("workflow", "lint.yml", "name: a\n"),
            flow_b = file("workflow", "lint.yml", "name: b\n"),
            flow_c = file("workflow", "my export.yaml", "name: c\n"),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let request = http(port, &body);
        let response = exchange_with(listener, request.as_bytes(), bins);
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.contains("kostencheck.md"), "{response}");
        assert!(response.contains("# log-bill teardown"), "{response}");
        assert!(response.contains("Total net: 0.30 USD"), "{response}");
    }

    #[test]
    fn a_workflow_must_be_yaml() {
        let body = format!(
            "{}{}{}--bound--\r\n",
            field("tool", "log-bill"),
            file("export", "app.log", "ERROR disk full\n"),
            file("workflow", "notes.txt", "nope\n"),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let response = exchange_on(listener, http(port, &body).as_bytes());
        assert!(response.contains("400 Bad Request"), "{response}");
        assert!(response.contains("workflow"), "{response}");
    }

    #[test]
    fn routes_that_are_not_the_report() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let missing = exchange_on(
            listener,
            format!("GET /nope HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
        assert!(missing.contains("404 Not Found"), "{missing}");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let wrong = exchange_on(
            listener,
            format!("GET /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
        assert!(wrong.contains("405 Method Not Allowed"), "{wrong}");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = exchange_on(
            listener,
            format!("GET / HTTP/1.1\r\nHost: example.com:{port}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
        assert!(host.contains("400 Bad Request"), "{host}");
        assert!(host.contains("host is niet deze computer"), "{host}");
    }

    #[cfg(unix)]
    struct ServeScratch(PathBuf);

    #[cfg(unix)]
    impl ServeScratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "cc-serve-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    #[cfg(unix)]
    impl Drop for ServeScratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn field(name: &str, value: &str) -> String {
        format!("--bound\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
    }

    fn file(name: &str, filename: &str, value: &str) -> String {
        format!(
            "--bound\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\r\n{value}\r\n"
        )
    }

    fn http(port: u16, body: &str) -> String {
        format!(
            "POST /report HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: multipart/form-data; boundary=bound\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// A `sh` stub that prints `json`. Unix only: it needs `sh` and a mode.
    #[cfg(unix)]
    fn script(dir: &std::path::Path, name: &str, json: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let json_path = dir.join(format!("{name}.json"));
        fs::write(&json_path, json).unwrap();
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\ncat '{}'\n", json_path.display())).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[cfg(unix)]
    const LOG_JSON: &str = r#"{
        "schema_version": 1,
        "tool": "log-bill",
        "estimate": "estimate: one sample",
        "inputs": {"paths": ["app.log"]},
        "window": {"seconds": 60, "factor": "1440"},
        "totals": {"lines": 2, "bytes": 40},
        "patterns": [{
            "id": "Pabc",
            "text": "disk full",
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

    fn exchange_on(listener: TcpListener, request: &[u8]) -> String {
        exchange_with(listener, request, Bins::default())
    }

    #[cfg(unix)]
    #[test]
    fn an_upload_cut_inside_a_multibyte_char_is_a_report() {
        for ch in ["é", "日", "😀"] {
            for start in [8189usize, 8190, 8191, 8192] {
                let scratch = ServeScratch::new();
                let stub = script(scratch.path(), "log-bill", LOG_JSON);
                let bins = Bins {
                    log_bill: Some(stub),
                    ..Bins::default()
                };
                let head = "{\"msg\":\"";
                let export = format!(
                    "{head}{}{ch}\"}}\n{{\"msg\":\"é 😀\"}}\n",
                    "a".repeat(start - head.len())
                );
                let body = format!(
                    "{}{}--bound--\r\n",
                    field("tool", "log-bill"),
                    file("export", "app.jsonl", &export)
                );
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let port = listener.local_addr().unwrap().port();
                let request = http(port, &body);
                let response = exchange_with(listener, request.as_bytes(), bins);
                assert!(response.contains("200 OK"), "{ch} at {start}: {response}");
                assert!(response.contains("# log-bill teardown"), "{response}");
            }
        }
    }

    fn exchange_with(listener: TcpListener, request: &[u8], bins: Bins) -> String {
        let addr = listener.local_addr().unwrap();
        let app = App {
            bins,
            port: addr.port(),
        };
        let server = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            handle(sock, &app).unwrap();
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(request).unwrap();
        let mut buf = Vec::new();
        let _ = client.read_to_end(&mut buf);
        server.join().unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }
}
