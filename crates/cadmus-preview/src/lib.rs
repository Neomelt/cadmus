//! 阅读预览渲染：把块序列输出为可阅读的 HTML。
//!
//! 预览即阅读器体验的最小预演：主题（日间/夜间/纸张）、字号、行宽、
//! 目录（由标题块生成）、封面与作者。CLI 与 App 共用本 crate。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadmus_core::{BitmapRef, Block, BlockKind, Document, Payload, Result};

/// 预览元信息。
pub struct PreviewMeta<'a> {
    pub title: &'a str,
    /// 作者（电子书适用）。
    pub byline: Option<&'a str>,
    /// 封面路径（可为 out_dir 内的绝对路径）。
    pub cover: Option<&'a str>,
}

const HEAD: &str = r#"<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
"#;

const STYLE: &str = r#"<style>
:root {
  --fs: 18px; --width: 40rem;
  --bg: #ffffff; --fg: #1a1a1a; --muted: #8a8a8a; --soft: rgba(127,127,127,.14);
}
body[data-theme="night"] { --bg: #101418; --fg: #cfd6dd; --muted: #7b8794; --soft: rgba(255,255,255,.08); }
body[data-theme="paper"] { --bg: #f6efe3; --fg: #3f342a; --muted: #9b8b77; --soft: rgba(120,90,50,.10); }
html, body { margin: 0; }
body { background: var(--bg); color: var(--fg); }
.toolbar {
  position: sticky; top: 0; z-index: 30;
  display: flex; gap: .4rem; align-items: center;
  padding: .45rem .7rem; background: var(--bg); border-bottom: 1px solid var(--soft);
  font-family: system-ui, sans-serif; font-size: 14px;
}
.toolbar button {
  font: inherit; padding: .3rem .65rem; cursor: pointer;
  border: 1px solid var(--soft); border-radius: 6px;
  background: transparent; color: var(--fg);
}
.toolbar .title { flex: 1; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
main {
  margin: 0 auto; padding: 2rem 1.25rem 5rem; max-width: var(--width);
  font-family: "Noto Serif CJK SC", "Source Han Serif SC", Georgia, "Songti SC", serif;
  font-size: var(--fs); line-height: 1.7;
}
main h1 { font-size: 1.5em; } main h2 { font-size: 1.25em; }
p { margin: .9em 0; }
.byline { color: var(--muted); margin-top: -.6em; }
img { max-width: 100%; height: auto; }
figure { margin: 1.4rem 0; text-align: center; }
figure img.formula { max-height: 8em; margin: 0 auto; }
img.cover { display: block; max-height: 60vh; margin: 0 auto 2rem; border-radius: 6px; box-shadow: 0 2px 14px rgba(0,0,0,.2); }
pre { overflow-x: auto; padding: .75rem 1rem; border-radius: 6px; background: var(--soft); font-size: .9em; line-height: 1.5; }
footer { margin-top: 3rem; color: var(--muted); font-size: .8em; text-align: center; }
#toc {
  position: fixed; top: 0; bottom: 0; left: 0; z-index: 40;
  width: min(78vw, 21rem); overflow: auto; padding: 1rem;
  background: var(--bg); border-right: 1px solid var(--soft);
  transform: translateX(-102%); transition: transform .18s ease;
  font-family: system-ui, sans-serif; font-size: 15px;
}
#toc.open { transform: none; }
#toc a { display: block; padding: .45rem .2rem; color: var(--fg); text-decoration: none; border-bottom: 1px solid var(--soft); }
#scrim { position: fixed; inset: 0; z-index: 35; background: rgba(0,0,0,.35); display: none; }
#scrim.open { display: block; }
</style>"#;

const SCRIPT: &str = r#"<script>
(function () {
  var root = document.documentElement;
  var body = document.body;
  var save = {};
  try { save = JSON.parse(localStorage.getItem('cadmus-preview') || '{}'); } catch (e) {}
  var settings = { fs: 18, width: 40, theme: 'day' };
  for (var key in settings) { if (save[key]) settings[key] = save[key]; }
  function persist() { try { localStorage.setItem('cadmus-preview', JSON.stringify(settings)); } catch (e) {} }
  function apply() {
    root.style.setProperty('--fs', settings.fs + 'px');
    root.style.setProperty('--width', settings.width + 'rem');
    body.setAttribute('data-theme', settings.theme);
    document.getElementById('theme-btn').textContent = ({ day: '日间', night: '夜间', paper: '纸张' })[settings.theme];
    persist();
  }
  document.getElementById('fs-minus').addEventListener('click', function () { settings.fs = Math.max(13, settings.fs - 1); apply(); });
  document.getElementById('fs-plus').addEventListener('click', function () { settings.fs = Math.min(28, settings.fs + 1); apply(); });
  document.getElementById('width-btn').addEventListener('click', function () { settings.width = settings.width >= 40 ? 32 : 40; apply(); });
  document.getElementById('theme-btn').addEventListener('click', function () {
    var order = ['day', 'night', 'paper'];
    settings.theme = order[(order.indexOf(settings.theme) + 1) % order.length];
    apply();
  });
  var toc = document.getElementById('toc');
  var scrim = document.getElementById('scrim');
  function closeToc() { toc.classList.remove('open'); scrim.classList.remove('open'); }
  document.getElementById('toc-btn').addEventListener('click', function () { toc.classList.add('open'); scrim.classList.add('open'); });
  scrim.addEventListener('click', closeToc);
  var nav = document.getElementById('toc-nav');
  var headings = document.querySelectorAll('main h1, main h2');
  for (var i = 0; i < headings.length; i++) {
    var h = headings[i];
    h.id = 'h' + i;
    var a = document.createElement('a');
    a.textContent = h.textContent;
    a.href = '#' + h.id;
    if (h.tagName === 'H2') a.style.paddingLeft = '1rem';
    a.addEventListener('click', closeToc);
    nav.appendChild(a);
  }
  if (headings.length === 0) nav.innerHTML = '<p style="color:var(--muted)">（没有可导航的标题）</p>';
  apply();
})();
</script>"#;

/// 渲染为 HTML 字符串；`base` 用于把位图路径转为相对路径（无位图时可为空路径）。
pub fn render_to_string(
    document: &Document,
    base: &Path,
    meta: &PreviewMeta<'_>,
) -> Result<String> {
    let mut html = String::new();
    html.push_str(HEAD);
    let _ = writeln!(html, "<title>{}</title>", escape(meta.title));
    html.push_str(STYLE);
    html.push_str("</head>\n<body data-theme=\"day\">\n");

    html.push_str("<div class=\"toolbar\">\n");
    html.push_str("<button id=\"toc-btn\">目录</button>\n");
    let _ = writeln!(html, "<span class=\"title\">{}</span>", escape(meta.title));
    html.push_str("<button id=\"theme-btn\">日间</button>\n");
    html.push_str("<button id=\"fs-minus\">A−</button>\n");
    html.push_str("<button id=\"fs-plus\">A+</button>\n");
    html.push_str("<button id=\"width-btn\">行宽</button>\n");
    html.push_str("</div>\n");

    html.push_str(
        "<div id=\"scrim\"></div>\n<aside id=\"toc\"><nav id=\"toc-nav\"></nav></aside>\n",
    );
    html.push_str("<main id=\"content\">\n");

    if let Some(cover) = meta.cover {
        let _ = writeln!(
            html,
            "<img class=\"cover\" src=\"{}\" alt=\"cover\">",
            escape(&relative(cover, base))
        );
    }
    if meta.cover.is_some() || meta.byline.is_some() {
        let _ = writeln!(html, "<h1>{}</h1>", escape(meta.title));
        if let Some(byline) = meta.byline {
            let _ = writeln!(html, "<p class=\"byline\">{}</p>", escape(byline));
        }
    }

    for block in &document.blocks {
        render_block(&mut html, block, base);
    }
    let _ = writeln!(
        html,
        "</main>\n<footer>cadmus 预览 · {} 个块</footer>",
        document.blocks.len()
    );
    html.push_str(SCRIPT);
    html.push_str("</body>\n</html>\n");
    Ok(html)
}

/// 渲染并写入 `<out_dir>/index.html`，位图以相对路径引用。
pub fn render(document: &Document, out_dir: &Path, meta: &PreviewMeta<'_>) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let html = render_to_string(document, out_dir, meta)?;
    let path = out_dir.join("index.html");
    std::fs::write(&path, html)?;
    Ok(path)
}

fn render_block(html: &mut String, block: &Block, base: &Path) {
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
                escape(&relative_src(bitmap, base))
            );
        }
        (_, Payload::Bitmap(bitmap)) => {
            let _ = writeln!(
                html,
                "<figure><img src=\"{}\" alt=\"figure\"></figure>",
                escape(&relative_src(bitmap, base))
            );
        }
    }
}

fn relative_src(bitmap: &BitmapRef, base: &Path) -> String {
    relative(&bitmap.path, base)
}

fn relative(path: &str, base: &Path) -> String {
    Path::new(path)
        .strip_prefix(base)
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadmus_core::BlockKind;

    #[test]
    fn renders_title_and_escapes_text() {
        let document = Document {
            doc_hash: 1,
            title: Some("书".into()),
            blocks: vec![Block::text(
                1,
                BlockKind::Paragraph,
                0,
                "正文 <b>不是标签</b> & 符号",
            )],
        };
        let html = render_to_string(
            &document,
            Path::new(""),
            &PreviewMeta {
                title: "书",
                byline: Some("作者"),
                cover: None,
            },
        )
        .unwrap();
        assert!(html.contains("<title>书</title>"));
        assert!(html.contains("正文 &lt;b&gt;不是标签&lt;/b&gt; &amp; 符号"));
        assert!(html.contains("作者"));
    }
}
