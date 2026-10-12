//! 阅读预览渲染：把块序列输出为可阅读的 HTML。
//!
//! 阅读器体验：目录、设置面板（字号/行距/行宽滑条、字体、主题）。
//! CLI 与 App 共用本 crate。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadmus_core::{Block, BlockKind, Document, Payload, Result};

/// 预览元信息。
pub struct PreviewMeta<'a> {
    pub title: &'a str,
    /// 作者（电子书适用）。
    pub byline: Option<&'a str>,
    /// 封面路径（可为 out_dir 内的绝对路径）。
    pub cover: Option<&'a str>,
    /// 位图/封面路径 → 可用的 src（如 data URL）；`None` 时使用相对 `base` 的路径。
    pub asset_src: Option<&'a dyn Fn(&str) -> String>,
}

const HEAD: &str = r#"<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
"#;

const STYLE: &str = r#"<style>
:root {
  --fs: 18px; --lh: 1.7; --width: 40rem;
  --font: "Noto Serif CJK SC", "Source Han Serif SC", "Songti SC", Georgia, serif;
  --bg: #ffffff; --fg: #1a1a1a; --muted: #8a8a8a; --soft: rgba(127,127,127,.14);
}
body[data-theme="sepia"] { --bg: #f6efe3; --fg: #3f342a; --muted: #9b8b77; --soft: rgba(120,90,50,.10); }
body[data-theme="green"] { --bg: #cce8cf; --fg: #26332a; --muted: #6b7d6f; --soft: rgba(40,80,50,.12); }
body[data-theme="night"] { --bg: #101418; --fg: #cfd6dd; --muted: #7b8794; --soft: rgba(255,255,255,.08); }
body[data-theme="black"] { --bg: #000000; --fg: #b9b9b9; --muted: #6f6f6f; --soft: rgba(255,255,255,.07); }
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
  font-family: var(--font); font-size: var(--fs); line-height: var(--lh);
}
main h1 { font-size: 1.5em; } main h2 { font-size: 1.25em; }
p { margin: .9em 0; }
.byline { color: var(--muted); margin-top: -.6em; }
.footnote { font-size: .92em; color: var(--muted); margin: 1.2rem 0; padding-top: .8rem; border-top: 1px solid var(--soft); }
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
#sheet {
  position: fixed; left: 0; right: 0; bottom: 0; z-index: 40;
  max-height: 72vh; overflow: auto;
  background: var(--bg); border-top: 1px solid var(--soft);
  border-radius: 14px 14px 0 0; padding: .9rem 1.1rem 1.6rem;
  transform: translateY(105%); transition: transform .2s ease;
  font-family: system-ui, sans-serif; font-size: 15px;
  box-shadow: 0 -8px 30px rgba(0,0,0,.18);
}
#sheet.open { transform: none; }
#sheet h3 { margin: .2rem 0 .8rem; font-size: .95rem; color: var(--muted); font-weight: 500; }
.srow { display: flex; align-items: center; gap: .8rem; margin: .85rem 0; }
.srow > label { width: 3rem; flex: none; color: var(--muted); }
.srow input[type="range"] { flex: 1; accent-color: var(--fg); }
.srow .val { width: 3.2rem; text-align: right; color: var(--muted); font-variant-numeric: tabular-nums; }
.chips { display: flex; flex-wrap: wrap; gap: .45rem; }
.chips button {
  font: inherit; padding: .3rem .75rem; cursor: pointer;
  border: 1px solid var(--soft); border-radius: 999px;
  background: transparent; color: var(--fg);
}
.chips button.on { border-color: var(--fg); background: var(--soft); }
#scrim { position: fixed; inset: 0; z-index: 35; background: rgba(0,0,0,.35); display: none; }
#scrim.open { display: block; }
</style>"#;

const SCRIPT: &str = r#"<script>
(function () {
  var root = document.documentElement;
  var body = document.body;

  var FONTS = {
    serif: '"Noto Serif CJK SC", "Source Han Serif SC", "Songti SC", Georgia, serif',
    sans: '-apple-system, "PingFang SC", "Noto Sans CJK SC", "Helvetica Neue", sans-serif',
    kai: '"Kaiti SC", "STKaiti", "KaiTi", "AR PL UKai CN", serif'
  };
  var defaults = { fs: 18, lh: 1.7, width: 40, font: 'serif', theme: 'day' };
  var settings = {};
  for (var key in defaults) settings[key] = defaults[key];
  try {
    var saved = JSON.parse(localStorage.getItem('cadmus-reader-settings') || '{}');
    for (var k2 in defaults) { if (saved[k2] !== undefined) settings[k2] = saved[k2]; }
  } catch (e) {}

  function persist() {
    try { localStorage.setItem('cadmus-reader-settings', JSON.stringify(settings)); } catch (e) {}
  }
  function apply() {
    root.style.setProperty('--fs', settings.fs + 'px');
    root.style.setProperty('--lh', settings.lh);
    root.style.setProperty('--width', settings.width + 'rem');
    root.style.setProperty('--font', FONTS[settings.font] || FONTS.serif);
    body.setAttribute('data-theme', settings.theme);
    document.getElementById('s-fs').value = settings.fs;
    document.getElementById('s-lh').value = settings.lh;
    document.getElementById('s-width').value = settings.width;
    document.getElementById('v-fs').textContent = settings.fs + 'px';
    document.getElementById('v-lh').textContent = settings.lh.toFixed(2);
    document.getElementById('v-width').textContent = settings.width + 'rem';
    markChips('c-font', 'font');
    markChips('c-theme', 'theme');
    persist();
  }
  function markChips(id, key) {
    var buttons = document.getElementById(id).querySelectorAll('button');
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].classList.toggle('on', buttons[i].dataset.value === settings[key]);
    }
  }
  function buildChips(id, items, key) {
    var wrap = document.getElementById(id);
    items.forEach(function (item) {
      var b = document.createElement('button');
      b.textContent = item[1];
      b.dataset.value = item[0];
      b.addEventListener('click', function () { settings[key] = item[0]; apply(); });
      wrap.appendChild(b);
    });
  }

  document.getElementById('s-fs').addEventListener('input', function () { settings.fs = +this.value; apply(); });
  document.getElementById('s-lh').addEventListener('input', function () { settings.lh = +this.value; apply(); });
  document.getElementById('s-width').addEventListener('input', function () { settings.width = +this.value; apply(); });

  buildChips('c-font', [['serif', '宋体'], ['sans', '黑体'], ['kai', '楷体']], 'font');
  buildChips('c-theme', [['day', '日间'], ['sepia', '米黄'], ['green', '护眼'], ['night', '深灰'], ['black', '纯黑']], 'theme');

  var scrim = document.getElementById('scrim');
  function closeAll() {
    document.getElementById('toc').classList.remove('open');
    document.getElementById('sheet').classList.remove('open');
    scrim.classList.remove('open');
  }
  function openOne(id) {
    closeAll();
    document.getElementById(id).classList.add('open');
    scrim.classList.add('open');
  }
  document.getElementById('toc-btn').addEventListener('click', function () { openOne('toc'); });
  document.getElementById('set-btn').addEventListener('click', function () { openOne('sheet'); });
  scrim.addEventListener('click', closeAll);

  var nav = document.getElementById('toc-nav');
  var headings = document.querySelectorAll('main h1, main h2');
  for (var i = 0; i < headings.length; i++) {
    var h = headings[i];
    h.id = 'h' + i;
    var a = document.createElement('a');
    a.textContent = h.textContent;
    a.href = '#' + h.id;
    if (h.tagName === 'H2') a.style.paddingLeft = '1rem';
    a.addEventListener('click', closeAll);
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
    html.push_str("<button id=\"set-btn\">Aa 设置</button>\n");
    html.push_str("</div>\n");

    html.push_str("<div id=\"scrim\"></div>\n");
    html.push_str("<aside id=\"toc\"><nav id=\"toc-nav\"></nav></aside>\n");
    html.push_str("<section id=\"sheet\">\n<h3>阅读设置</h3>\n");
    html.push_str("<div class=\"srow\"><label>字号</label><input id=\"s-fs\" type=\"range\" min=\"13\" max=\"30\" step=\"1\"><span class=\"val\" id=\"v-fs\"></span></div>\n");
    html.push_str("<div class=\"srow\"><label>行距</label><input id=\"s-lh\" type=\"range\" min=\"1.3\" max=\"2.2\" step=\"0.05\"><span class=\"val\" id=\"v-lh\"></span></div>\n");
    html.push_str("<div class=\"srow\"><label>行宽</label><input id=\"s-width\" type=\"range\" min=\"26\" max=\"56\" step=\"1\"><span class=\"val\" id=\"v-width\"></span></div>\n");
    html.push_str(
        "<div class=\"srow\"><label>字体</label><div class=\"chips\" id=\"c-font\"></div></div>\n",
    );
    html.push_str(
        "<div class=\"srow\"><label>主题</label><div class=\"chips\" id=\"c-theme\"></div></div>\n",
    );
    html.push_str("</section>\n");

    html.push_str("<main id=\"content\">\n");

    if let Some(cover) = meta.cover {
        let src = src_for(cover, meta, base);
        if !src.is_empty() {
            let _ = writeln!(
                html,
                "<img class=\"cover\" src=\"{}\" alt=\"cover\">",
                escape(&src)
            );
        }
    }
    if meta.cover.is_some() || meta.byline.is_some() {
        let _ = writeln!(html, "<h1>{}</h1>", escape(meta.title));
        if let Some(byline) = meta.byline {
            let _ = writeln!(html, "<p class=\"byline\">{}</p>", escape(byline));
        }
    }

    for block in &document.blocks {
        render_block(&mut html, block, base, meta);
    }
    let _ = writeln!(
        html,
        "</main>\n<footer>cadmus · {} 个块</footer>",
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

fn render_block(html: &mut String, block: &Block, base: &Path, meta: &PreviewMeta<'_>) {
    match (&block.kind, &block.payload) {
        (BlockKind::Heading { level }, Payload::Text(text)) => {
            let tag = if *level <= 1 { "h1" } else { "h2" };
            let _ = writeln!(html, "<{tag}>{}</{tag}>", escape(text));
        }
        (BlockKind::Code, Payload::Text(text)) => {
            let _ = writeln!(html, "<pre>{}</pre>", escape(text));
        }
        (BlockKind::Footnote, Payload::Text(text)) => {
            let _ = writeln!(html, "<aside class=\"footnote\">{}</aside>", escape(text));
        }
        (_, Payload::Text(text)) => {
            let _ = writeln!(html, "<p>{}</p>", escape(text));
        }
        (BlockKind::Formula, Payload::Bitmap(bitmap)) => {
            let src = src_for(&bitmap.path, meta, base);
            if !src.is_empty() {
                let _ = writeln!(
                    html,
                    "<figure><img class=\"formula\" src=\"{}\" alt=\"formula\"></figure>",
                    escape(&src)
                );
            }
        }
        (_, Payload::Bitmap(bitmap)) => {
            let src = src_for(&bitmap.path, meta, base);
            if !src.is_empty() {
                let _ = writeln!(
                    html,
                    "<figure><img src=\"{}\" alt=\"figure\"></figure>",
                    escape(&src)
                );
            }
        }
    }
}

/// 位图路径 → src：优先使用调用方提供的解析器，否则回退为相对 `base` 的路径。
/// 返回空串表示该位图不可用（调用方应整体省略，避免裂图）。
fn src_for(path: &str, meta: &PreviewMeta<'_>, base: &Path) -> String {
    match meta.asset_src {
        Some(resolver) => resolver(path),
        None => relative(path, base),
    }
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
    use cadmus_core::{BitmapRef, BlockKind, Payload};

    fn base_meta<'a>(title: &'a str) -> PreviewMeta<'a> {
        PreviewMeta {
            title,
            byline: None,
            cover: None,
            asset_src: None,
        }
    }

    #[test]
    fn empty_resolved_source_omits_figure() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block {
                id: 2,
                kind: BlockKind::Figure,
                order: 0,
                page_range: None,
                payload: Payload::Bitmap(BitmapRef {
                    path: "assets/big.png".into(),
                    width_px: 0,
                    height_px: 0,
                }),
            }],
        };
        let resolver = |_path: &str| String::new();
        let meta = PreviewMeta {
            asset_src: Some(&resolver),
            ..base_meta("t")
        };
        let html = render_to_string(&document, Path::new(""), &meta).unwrap();
        assert!(
            !html.contains("<figure") && !html.contains("assets/big.png"),
            "解析为空的位图应整体省略，不出现裂图"
        );
    }

    #[test]
    fn renders_footnote_blocks_as_asides() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block::text(3, BlockKind::Footnote, 0, "脚注正文：解释。")],
        };
        let html = render_to_string(&document, Path::new(""), &base_meta("t")).unwrap();
        assert!(
            html.contains("<aside class=\"footnote\">脚注正文：解释。</aside>"),
            "脚注块应渲染为 aside.footnote"
        );
    }

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
                ..base_meta("书")
            },
        )
        .unwrap();
        assert!(html.contains("<title>书</title>"));
        assert!(html.contains("正文 &lt;b&gt;不是标签&lt;/b&gt; &amp; 符号"));
        assert!(html.contains("作者"));
        assert!(html.contains("s-fs"), "缺字号滑条");
        assert!(html.contains("c-theme"), "缺主题选项");
        assert!(html.contains("楷体"), "缺字体选项");
    }

    #[test]
    fn asset_resolver_rewrites_bitmap_sources() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block {
                id: 2,
                kind: BlockKind::Figure,
                order: 0,
                page_range: None,
                payload: Payload::Bitmap(BitmapRef {
                    path: "assets/p1.png".into(),
                    width_px: 10,
                    height_px: 10,
                }),
            }],
        };
        let resolver = |path: &str| format!("data:demo/{path}");
        let meta = PreviewMeta {
            asset_src: Some(&resolver),
            ..base_meta("t")
        };
        let html = render_to_string(&document, Path::new(""), &meta).unwrap();
        assert!(
            html.contains("data:demo/assets/p1.png"),
            "位图 src 应经过 asset_src 解析器"
        );
        assert!(
            !html.contains("src=\"assets/p1.png\""),
            "不应回退到相对路径"
        );
    }
}
