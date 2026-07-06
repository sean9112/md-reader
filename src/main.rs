use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::exit;

use comrak::{markdown_to_html, ExtensionOptions, Options, RenderOptions};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

const TEMPLATE: &str = include_str!("template.html");

/// Front-end libraries, inlined into the generated page (only when the
/// document needs them) so it works offline with no server and no network —
/// a plain file:// document.
const HIGHLIGHT_JS: &str = include_str!("../vendor/highlight.min.js");
const MATHJAX_JS: &str = include_str!("../vendor/mathjax-tex-svg-full.js");
const MERMAID_JS: &str = include_str!("../vendor/mermaid.min.js");
const HLJS_LIGHT_CSS: &str = include_str!("../vendor/hljs-github.min.css");
const HLJS_DARK_CSS: &str = include_str!("../vendor/hljs-github-dark.min.css");

struct Cli {
    target: PathBuf,
    output: Option<PathBuf>,
    no_open: bool,
}

fn main() {
    let cli = parse_cli();
    let (base_dir, doc) = resolve_target(&cli.target);

    let src = fs::read_to_string(&doc)
        .unwrap_or_else(|e| die(1, format!("cannot read {}: {e}", doc.display())));
    let page = build_page(&doc, &base_dir, &src);

    let out_path = cli.output.unwrap_or_else(|| default_out_path(&doc));
    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&out_path, page)
        .unwrap_or_else(|e| die(1, format!("cannot write {}: {e}", out_path.display())));

    if cli.no_open {
        // Print the path so callers can pipe it or open it themselves.
        println!("{}", out_path.display());
    } else if let Err(e) = open::that(&out_path) {
        die(
            1,
            format!(
                "cannot open browser: {e}\nrendered file: {}",
                out_path.display()
            ),
        );
    }
}

fn parse_cli() -> Cli {
    fn set_target(slot: &mut Option<PathBuf>, arg: String) {
        if slot.is_some() {
            die(2, format!("unexpected extra argument: {arg}"));
        }
        *slot = Some(PathBuf::from(arg));
    }

    let mut output = None;
    let mut no_open = false;
    let mut target = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-open" => no_open = true,
            "-o" | "--output" => match args.next() {
                // A flag-looking value is almost certainly a mistake
                // (`-o --no-open file.md`), not an intended filename.
                Some(v) if !v.starts_with('-') => output = Some(PathBuf::from(v)),
                _ => die(2, "--output needs a path"),
            },
            "--help" | "-h" => {
                print_help();
                exit(0);
            }
            // Everything after `--` is a target, even if it starts with `-`.
            "--" => args.by_ref().for_each(|a| set_target(&mut target, a)),
            _ if arg.starts_with('-') => die(2, format!("unknown flag: {arg}")),
            _ => set_target(&mut target, arg),
        }
    }

    Cli {
        target: target.unwrap_or_else(|| die(2, "no markdown file given (see --help)")),
        output,
        no_open,
    }
}

/// Canonicalize the CLI target into (asset base directory, document path).
/// A directory target opens its README.md/index.md.
fn resolve_target(target: &Path) -> (PathBuf, PathBuf) {
    let target = target
        .canonicalize()
        .unwrap_or_else(|e| die(1, format!("cannot open {}: {e}", target.display())));

    if target.is_dir() {
        let doc = ["README.md", "readme.md", "index.md"]
            .iter()
            .map(|n| target.join(n))
            .find(|p| p.is_file())
            .unwrap_or_else(|| die(1, format!("{} has no README.md/index.md", target.display())));
        (target, doc)
    } else {
        let dir = target.parent().unwrap_or(Path::new("/")).to_path_buf();
        (dir, target)
    }
}

/// Render the markdown source into a complete, self-contained HTML page.
fn build_page(doc: &Path, base_dir: &Path, src: &str) -> String {
    let title = doc.file_name().unwrap_or_default().to_string_lossy();
    // Rewrite relative asset URLs to absolute file:// so images load from the
    // document's own directory even though the page lives in a temp file.
    let body = absolutize_urls(&render_markdown(src), base_dir);

    // Only inline the libraries the document actually uses, so a plain note
    // stays small instead of carrying multi-megabyte payloads. The needles
    // end in `="`, which comrak escapes to `=&quot;` in prose and code, so
    // only genuine markup (comrak's own output or raw HTML) can match.
    let mermaid_pres = body.matches("<pre lang=\"mermaid\">").count();
    let has_mermaid = mermaid_pres > 0 || body.contains("class=\"mermaid\"");
    let has_math = body.contains("data-math-style=\"");
    let has_code = body.matches("<pre>").count() + body.matches("<pre ").count() > mermaid_pres;

    let script = |cond: bool, js: &str| {
        if cond {
            format!("<script>{}</script>", guard_script(js))
        } else {
            String::new()
        }
    };
    let hljs_css = format!(
        "<style media=\"(prefers-color-scheme: light)\">{HLJS_LIGHT_CSS}</style>\n\
         <style media=\"(prefers-color-scheme: dark)\">{HLJS_DARK_CSS}</style>"
    );
    // Injected only alongside mermaid.js: this rule hides diagram sources
    // until they render, so it must never apply when nothing will render them.
    let mermaid_css = "<style>.mermaid{text-align:center}\
                       .mermaid:not([data-processed]){visibility:hidden}</style>";

    fill(
        TEMPLATE,
        &[
            ("TITLE", &html_escape(&title)),
            ("HLJS_CSS", if has_code { &hljs_css } else { "" }),
            ("MERMAID_CSS", if has_mermaid { mermaid_css } else { "" }),
            ("BODY", &body),
            ("MATHJAX", &script(has_math, MATHJAX_JS)),
            ("HIGHLIGHT", &script(has_code, HIGHLIGHT_JS)),
            ("MERMAID", &script(has_mermaid, MERMAID_JS)),
        ],
    )
}

/// Single-pass template renderer. Each `{{KEY}}` in the template is replaced
/// by its value; substituted values are never rescanned, so document content
/// that happens to contain a placeholder cannot trigger a second expansion.
fn fill(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            if let Some((_, v)) = vars.iter().find(|(k, _)| *k == &after[..end]) {
                out.push_str(&rest[..start]);
                out.push_str(v);
                rest = &after[end + 2..];
                continue;
            }
        }
        // unknown key or no closing braces: copy the `{{` through and go on
        out.push_str(&rest[..start + 2]);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Neutralise any `</script` that would prematurely close the tag the library
/// is inlined into (`<\/` is equivalent inside JS).
fn guard_script(s: &str) -> String {
    s.replace("</script", "<\\/script")
}

/// A hidden per-document temp file, e.g. $TMPDIR/md-reader/3f2a…-notes.html.
/// The path is derived from the source so re-opening the same file overwrites
/// its previous render instead of piling up.
fn default_out_path(doc: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    doc.hash(&mut hasher);
    let stem = doc
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    env::temp_dir()
        .join("md-reader")
        .join(format!("{:016x}-{stem}.html", hasher.finish()))
}

/// Percent-encode set for turning a filesystem path into a file:// URL. Keeps
/// `/` as the separator; encodes spaces and characters with URL or HTML
/// meaning (`&`, `'`, `"`), so the result is safe inside a quoted attribute.
const PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'\'')
    .add(b'#')
    .add(b'?')
    .add(b'%')
    .add(b'&')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^')
    .add(b'[')
    .add(b']');

fn file_url(p: &Path) -> String {
    format!(
        "file://{}",
        utf8_percent_encode(&p.to_string_lossy(), PATH_SET)
    )
}

/// Rewrite relative `src`/`href` values in rendered HTML to absolute file://
/// URLs rooted at `base_dir`, leaving anchors, absolute paths and full URLs
/// untouched. Without this, images referenced relatively would 404 because
/// the page is served from a temp directory.
fn absolutize_urls(html: &str, base_dir: &Path) -> String {
    let mut out = html.to_string();
    for attr in ["src", "href"] {
        for quote in ['"', '\''] {
            out = rewrite_attr(&out, attr, quote, base_dir);
        }
    }
    out
}

fn rewrite_attr(html: &str, attr: &str, quote: char, base_dir: &Path) -> String {
    let needle = format!("{attr}={quote}");
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    while let Some(rel) = html[pos..].find(&needle) {
        let at = pos + rel;
        let val_start = at + needle.len();
        out.push_str(&html[pos..val_start]);
        pos = val_start;

        // Only rewrite a genuine attribute: preceded by whitespace and inside
        // an open tag (a `<` more recent than any `>`). This skips look-alikes
        // in raw-HTML text, JS strings and other attributes' values.
        // inside an open tag = the nearest angle bracket looking back is a `<`
        let before = &html[..at];
        let in_tag = before
            .rfind(['<', '>'])
            .is_some_and(|i| before[i..].starts_with('<'));
        if !in_tag || !before.ends_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }

        if let Some(end) = html[val_start..].find(quote) {
            let val = &html[val_start..val_start + end];
            // A value spilling across markup means the quote never closed;
            // leave it alone rather than swallow the following tags.
            if !val.contains(['<', '>', '\n']) {
                out.push_str(&absolutize(val, base_dir));
                pos = val_start + end; // closing quote copied on the next push
            }
        }
    }
    out.push_str(&html[pos..]);
    out
}

fn absolutize(val: &str, base_dir: &Path) -> String {
    let v = val.trim();
    if v.is_empty()
        || v.starts_with('#')
        || v.starts_with('/')
        || v.contains("://")
        || v.starts_with("data:")
        || v.starts_with("mailto:")
        || v.starts_with("tel:")
        || v.starts_with("javascript:")
    {
        return val.to_string();
    }
    // Keep any ?query / #fragment tail attached to the rewritten path.
    let (path_part, tail) = v.split_at(v.find(['#', '?']).unwrap_or(v.len()));
    // comrak emits attribute-escaped, percent-encoded destinations
    // (`my pic.png` → `my%20pic.png`); undo both to get the real filesystem
    // path, then encode exactly once when building the file:// URL.
    let path_part = path_part.replace("&amp;", "&");
    let decoded = percent_decode_str(&path_part).decode_utf8_lossy();
    format!("{}{tail}", file_url(&base_dir.join(&*decoded)))
}

fn render_markdown(src: &str) -> String {
    // Multi-line $$ … $$ blocks are pulled out before parsing: their content
    // (bare `=` lines, `\\` row breaks, `_`, `*`…) would otherwise be eaten by
    // Markdown syntax. They are re-inserted afterwards as display-math spans.
    // Tokens embed a hash of the source, so a document can never contain its
    // own placeholder and trigger a bogus replacement.
    let mut hasher = DefaultHasher::new();
    src.hash(&mut hasher);
    let key = hasher.finish();
    let (src, math_blocks) = protect_display_math(src, key);

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
        html = html.replace(&math_token(key, i), &span);
    }
    html
}

fn math_token(key: u64, i: usize) -> String {
    format!("mdreadermath{key:016x}x{i}token")
}

/// Replace multi-line `$$ … $$` blocks (outside code fences) with opaque
/// placeholder tokens, returning the rewritten source and the captured TeX.
fn protect_display_math(src: &str, key: u64) -> (String, Vec<String>) {
    let mut out = String::with_capacity(src.len());
    let mut blocks = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut lines = src.lines();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        let indent = &line[..line.len() - line.trim_start().len()];
        // 4+ columns of indentation makes an indented code block, not math.
        let indent_cols: usize = indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();

        if let Some((ch, len)) = fence {
            if trimmed.len() >= len && trimmed.chars().all(|c| c == ch) {
                fence = None;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let ch = trimmed.chars().next().unwrap();
            fence = Some((ch, trimmed.chars().take_while(|c| *c == ch).count()));
        } else if trimmed == "$$" && indent_cols < 4 {
            let mut body = String::new();
            let mut closed = false;
            for l in lines.by_ref() {
                if l.trim() == "$$" {
                    closed = true;
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            if closed {
                out.push_str(indent);
                out.push_str(&math_token(key, blocks.len()));
                out.push('\n');
                blocks.push(body);
            } else {
                // unterminated: emit everything unchanged
                out.push_str(line);
                out.push('\n');
                out.push_str(&body);
            }
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }
    (out, blocks)
}

fn html_escape(s: &str) -> String {
    // `&` first, so the other escapes aren't themselves re-escaped
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn die(code: i32, msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    exit(code);
}

fn print_help() {
    println!(
        "md-reader — Markdown reader with LaTeX (MathJax), Mermaid and full GFM rendering

USAGE:
    md-reader <FILE.md | DIR> [--output <FILE.html>] [--no-open]

OPTIONS:
    -o, --output <FILE>   Write the HTML here instead of a temp file
        --no-open         Print the generated HTML path instead of opening it
    -h, --help            Show this help

The document is rendered to a single self-contained HTML file (styles and
scripts inlined, heavy libraries only when the document uses them) and opened
in your browser via file:// — no server, no open port, no background process.
Relative images and assets load from the document's own directory. LaTeX,
Mermaid and syntax highlighting work offline."
    );
}
