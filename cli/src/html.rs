//! HTML 预览渲染：把块序列输出为可读的 `index.html`（CR-1 人工复核用）。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadmus_core::{BitmapRef, Block, BlockKind, Document, Payload, Result};

const STYLE: &str = r#"
:root { color-scheme: light dark; }
body {
  margin: 0 auto; padding: 2rem 1rem 4rem;
  max-width: 42rem;
  font-family: "Noto Serif CJK SC", "Source Han Serif SC", Georgia, "Songti SC", serif;
  line-height: 1.7; font-size: 17px;
}
h1 { font-size: 1.5em; } h2 { font-size: 1.25em; }
p { margin: 0.9em 0; }
img { max-width: 100%; height: auto; }
figure { margin: 1.4rem 0; text-align: center; }
figure img.formula { max-height: 8em; }
pre {
  overflow-x: auto; padding: .75rem 1rem; border-radius: 6px;
  background: rgba(127, 127, 127, .12);
  font-size: .9em; line-height: 1.5;
}
footer { margin-top: 3rem; opacity: .5; font-size: .8em; }
"#;

/// 渲染为 `<out_dir>/index.html`，位图以相对路径引用。
pub fn render(document: &Document, out_dir: &Path, title: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"zh\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(html, "<title>{}</title>", escape(title));
    let _ = writeln!(html, "<style>{STYLE}</style>\n</head>\n<body>");
    for block in &document.blocks {
        render_block(&mut html, block, out_dir);
    }
    let _ = writeln!(
        html,
        "<footer>cadmus-cli 预览 · {} 个块</footer>",
        document.blocks.len()
    );
    html.push_str("</body>\n</html>\n");

    let path = out_dir.join("index.html");
    std::fs::write(&path, html)?;
    Ok(path)
}

fn render_block(html: &mut String, block: &Block, out_dir: &Path) {
    match (&block.kind, &block.payload) {
        (BlockKind::Heading { level }, Payload::Text(text)) => {
            let tag = if *level <= 1 { "h1" } else { "h2" };
            let _ = writeln!(html, "<{tag}>{}</{tag}>", escape(text));
        }
        (BlockKind::Code, Payload::Text(text)) => {
            let _ = writeln!(html, "<pre>{}</pre>", escape(text));
        }
        (_, Payload::Text(text)) => {
            let _ = writeln!(html, "<p>{}</p>", escape(text));
        }
        (BlockKind::Formula, Payload::Bitmap(bitmap)) => {
            let _ = writeln!(
                html,
                "<figure><img class=\"formula\" src=\"{}\" alt=\"formula\"></figure>",
                escape(&relative_src(bitmap, out_dir))
            );
        }
        (_, Payload::Bitmap(bitmap)) => {
            let _ = writeln!(
                html,
                "<figure><img src=\"{}\" alt=\"figure\"></figure>",
                escape(&relative_src(bitmap, out_dir))
            );
        }
    }
}

fn relative_src(bitmap: &BitmapRef, out_dir: &Path) -> String {
    Path::new(&bitmap.path)
        .strip_prefix(out_dir)
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|_| bitmap.path.clone())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
