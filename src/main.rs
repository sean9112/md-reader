use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::UNIX_EPOCH;

use comrak::{markdown_to_html, ExtensionOptions, Options, RenderOptions};
use percent_encoding::percent_decode_str;
use tiny_http::{Header, Method, Response, Server};

const TEMPLATE: &str = include_str!("template.html");

/// Front-end libraries embedded in the binary so the reader works offline.
const VENDOR: &[(&str, &[u8], &str)] = &[
    (
        "mathjax.js",
        include_bytes!("../vendor/mathjax-tex-svg-full.js"),
        "text/javascript",
    ),
    (
        "mermaid.js",
        include_bytes!("../vendor/mermaid.min.js"),
        "text/javascript",
    ),
    (
        "highlight.js",
        include_bytes!("../vendor/highlight.min.js"),
        "text/javascript",
    ),
    (
        "hljs-github.css",
        include_bytes!("../vendor/hljs-github.min.css"),
        "text/css",
    ),
    (
        "hljs-github-dark.css",
        include_bytes!("../vendor/hljs-github-dark.min.css"),
        "text/css",
    ),
];

struct App {
    /// Directory used as the web root (parent of the opened file).
    base_dir: PathBuf,
    /// Path of the initially opened document, relative to base_dir.
    index_doc: PathBuf,
}

fn main() {
    let mut port: u16 = 8787;
    let mut no_open = false;
    let mut target: Option<PathBuf> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                let v = args.next().unwrap_or_default();
                port = v
                    .parse()
                    .unwrap_or_else(|_| die(2, &format!("invalid port: {v}")));
            }
            "--no-open" => no_open = true,
            "--help" | "-h" => return print_help(),
            _ if arg.starts_with('-') => die(2, &format!("unknown flag: {arg}")),
            _ => target = Some(PathBuf::from(arg)),
        }
    }

    let target = target.unwrap_or_else(|| die(2, "no markdown file given (see --help)"));
    let target = target
        .canonicalize()
        .unwrap_or_else(|e| die(1, &format!("cannot open {}: {e}", target.display())));

    let (base_dir, index_doc) = if target.is_dir() {
        let doc = ["README.md", "readme.md", "index.md"]
            .iter()
            .map(|n| target.join(n))
            .find(|p| p.is_file())
            .unwrap_or_else(|| {
                die(
                    1,
                    &format!("{} has no README.md/index.md", target.display()),
                )
            });
        let rel = doc.strip_prefix(&target).unwrap().to_path_buf();
        (target, rel)
    } else {
        let dir = target.parent().unwrap_or(Path::new("/")).to_path_buf();
        let rel = target.strip_prefix(&dir).unwrap().to_path_buf();
        (dir, rel)
    };
    let app = App {
        base_dir,
        index_doc,
    };

    let server = Server::http(("127.0.0.1", port))
        .unwrap_or_else(|e| die(1, &format!("cannot bind 127.0.0.1:{port}: {e}")));
    let url = format!("http://127.0.0.1:{port}/");
    println!(
        "md-reader: serving {} at {url}  (Ctrl-C to quit)",
        app.base_dir.join(&app.index_doc).display()
    );
    if !no_open {
        let _ = open::that(&url);
    }

    for request in server.incoming_requests() {
        let response = handle(&app, request.method(), request.url());
        let _ = request.respond(response);
    }
}

fn die(code: i32, msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(code);
}

fn print_help() {
    println!(
        "md-reader — Markdown reader with LaTeX (MathJax), Mermaid and full GFM rendering

USAGE:
    md-reader <FILE.md | DIR> [--port <PORT>] [--no-open]

OPTIONS:
    -p, --port <PORT>   Port to listen on (default: 8787)
        --no-open       Do not open the browser automatically
    -h, --help          Show this help

Relative links to other .md files are rendered too; images and other
assets are served from the document's directory. The page auto-reloads
when the file changes on disk."
    );
}

type Resp = Response<Cursor<Vec<u8>>>;

fn handle(app: &App, method: &Method, url: &str) -> Resp {
    if *method != Method::Get && *method != Method::Head {
        return respond(405, "text/plain", "method not allowed".into());
    }

    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let decoded = percent_decode_str(path).decode_utf8_lossy().to_string();

    if let Some(name) = decoded.strip_prefix("/__vendor/") {
        return match VENDOR.iter().find(|(n, _, _)| *n == name) {
            Some((_, bytes, ct)) => {
                let mut resp = respond(200, ct, bytes.to_vec());
                resp.add_header(header("Cache-Control", "max-age=86400"));
                resp
            }
            None => respond(404, "text/plain", "not found".into()),
        };
    }

    if decoded == "/__mtime" {
        let rel = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("path="))
            .unwrap_or("");
        let rel = percent_decode_str(rel).decode_utf8_lossy();
        return match resolve(app, &rel) {
            Some(p) => respond(200, "text/plain", mtime_string(&p).into_bytes()),
            None => respond(404, "text/plain", "not found".into()),
        };
    }

    let rel = if decoded == "/" {
        app.index_doc.to_string_lossy().into_owned()
    } else {
        decoded.trim_start_matches('/').to_string()
    };
    let Some(full) = resolve(app, &rel) else {
        return respond(404, "text/plain", "not found".into());
    };

    let is_md = full
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    if !is_md {
        return match fs::read(&full) {
            Ok(bytes) => respond(200, content_type(&full), bytes),
            Err(e) => respond(404, "text/plain", format!("not found: {e}").into_bytes()),
        };
    }

    match fs::read_to_string(&full) {
        Ok(src) => {
            let title = full.file_name().unwrap_or_default().to_string_lossy();
            let page = TEMPLATE
                .replace("{{TITLE}}", &html_escape(&title))
                .replace("{{DOC_PATH}}", &html_escape(&rel))
                .replace("{{MTIME}}", &mtime_string(&full))
                .replace("{{BODY}}", &render_markdown(&src));
            respond(200, "text/html; charset=utf-8", page.into_bytes())
        }
        Err(e) => respond(
            500,
            "text/plain",
            format!("cannot read file: {e}").into_bytes(),
        ),
    }
}

/// Resolve a URL path against base_dir, refusing anything that escapes it.
fn resolve(app: &App, rel: &str) -> Option<PathBuf> {
    let canon = app.base_dir.join(rel).canonicalize().ok()?;
    (canon.starts_with(&app.base_dir) && canon.is_file()).then_some(canon)
}

fn render_markdown(src: &str) -> String {
    // Multi-line $$ … $$ blocks are pulled out before parsing: their content
    // (bare `=` lines, `\\` row breaks, `_`, `*`…) would otherwise be eaten by
    // Markdown syntax. They are re-inserted afterwards as display-math spans.
    let (src, math_blocks) = protect_display_math(src);

    let options = Options {
        extension: ExtensionOptions {
            strikethrough: true,
            subscript: true,
            table: true,
            autolink: true,
            tasklist: true,
            superscript: true,
            header_ids: Some(String::new()),
            footnotes: true,
            description_lists: true,
            front_matter_delimiter: Some("---".into()),
            multiline_block_quotes: true,
            alerts: true,
            math_dollars: true,
            math_code: true,
            shortcodes: true,
            underline: true,
            ..ExtensionOptions::default()
        },
        render: RenderOptions {
            unsafe_: true, // allow raw HTML embedded in the document
            github_pre_lang: true,
            ..RenderOptions::default()
        },
        ..Options::default()
    };

    let mut html = markdown_to_html(&src, &options);
    for (i, tex) in math_blocks.iter().enumerate() {
        let span = format!(
            "<span data-math-style=\"display\">{}</span>",
            html_escape(tex)
        );
        html = html.replace(&math_token(i), &span);
    }
    html
}

fn math_token(i: usize) -> String {
    format!("mdreaderdisplaymath{i}endtoken")
}

/// Replace multi-line `$$ … $$` blocks (outside code fences) with opaque
/// placeholder tokens, returning the rewritten source and the captured TeX.
fn protect_display_math(src: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(src.len());
    let mut blocks = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut lines = src.lines();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if let Some((ch, len)) = fence {
            if trimmed.len() >= len && trimmed.chars().all(|c| c == ch) {
                fence = None;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let ch = trimmed.chars().next().unwrap();
            fence = Some((ch, trimmed.chars().take_while(|c| *c == ch).count()));
        } else if trimmed == "$$" {
            let mut body = String::new();
            let mut consumed = Vec::new();
            let closed = lines.by_ref().any(|l| {
                consumed.push(l);
                l.trim() == "$$"
            });
            if closed {
                consumed.pop();
                for l in consumed {
                    body.push_str(l);
                    body.push('\n');
                }
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(indent);
                out.push_str(&math_token(blocks.len()));
                out.push('\n');
                blocks.push(body);
                continue;
            }
            // unterminated: emit everything unchanged
            out.push_str(line);
            out.push('\n');
            for l in consumed {
                out.push_str(l);
                out.push('\n');
            }
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }
    (out, blocks)
}

fn mtime_string(p: &Path) -> String {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| format!("{}.{}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_else(|| "0".into())
}

fn content_type(p: &Path) -> &'static str {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

fn header(key: &str, value: &str) -> Header {
    Header::from_bytes(key, value).expect("valid header")
}

fn respond(status: u16, content_type: &str, body: Vec<u8>) -> Resp {
    let mut resp = Response::from_data(body).with_status_code(status);
    resp.add_header(header("Content-Type", content_type));
    resp
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
