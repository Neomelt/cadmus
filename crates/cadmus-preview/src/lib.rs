//! 阅读预览渲染：把块序列输出为可阅读的 HTML。
//!
//! 阅读器体验：目录、设置面板（字号/行距/行宽滑条、字体、主题）。
//! CLI 与 App 共用本 crate。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cadmus_core::{Block, BlockKind, Document, Payload, Result};

/// 批注标记：某块内的字符区间（半开区间，按字符计）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// 目标块的稳定 id（十进制字符串，避免 JS 大整数精度问题）。
    pub block_id: String,
    pub start: u32,
    pub end: u32,
    /// 批注 id（`<mark data-ann>`）。
    pub id: String,
    /// 颜色名（yellow/green/blue/pink）。
    pub color: String,
    /// 附着笔记（显示小标记并供编辑预填）。
    pub note: Option<String>,
}

/// 预览元信息。
pub struct PreviewMeta<'a> {
    pub title: &'a str,
    /// 作者（电子书适用）。
    pub byline: Option<&'a str>,
    /// 封面路径（可为 out_dir 内的绝对路径）。
    pub cover: Option<&'a str>,
    /// 位图/封面路径 → 可用的 src（如 data URL）；`None` 时使用相对 `base` 的路径。
    pub asset_src: Option<&'a dyn Fn(&str) -> String>,
    /// 批注标记（按块内字符区间渲染高亮）。
    pub marks: &'a [Mark],
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
  --pad: 1.25rem; --align: left;
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
  margin: 0 auto; padding: 2rem var(--pad) 5rem; max-width: var(--width);
  font-family: var(--font); font-size: var(--fs); line-height: var(--lh);
  text-align: var(--align);
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
mark.hl { border-radius: 3px; padding: 0 .06em; }
mark.hl-yellow { background: rgba(255,214,64,.45); }
mark.hl-green { background: rgba(126,217,87,.40); }
mark.hl-blue { background: rgba(94,168,255,.40); }
mark.hl-pink { background: rgba(255,143,195,.42); }
body[data-theme="night"] mark.hl-yellow, body[data-theme="black"] mark.hl-yellow { background: rgba(255,214,64,.28); }
body[data-theme="night"] mark.hl-green, body[data-theme="black"] mark.hl-green { background: rgba(126,217,87,.24); }
body[data-theme="night"] mark.hl-blue, body[data-theme="black"] mark.hl-blue { background: rgba(94,168,255,.26); }
body[data-theme="night"] mark.hl-pink, body[data-theme="black"] mark.hl-pink { background: rgba(255,143,195,.26); }
mark.hl.has-note::after { content: "✎"; font-size: .68em; vertical-align: super; margin-left: .15em; color: var(--muted); }
.flash { animation: cadmus-flash 1.4s ease; }
@keyframes cadmus-flash { 0% { box-shadow: 0 0 0 3px rgba(255,214,64,.9); } 100% { box-shadow: none; } }
#ann-bar {
  position: fixed; z-index: 45; display: none; gap: .35rem; align-items: center;
  padding: .35rem .45rem; border-radius: 10px;
  background: var(--bg); border: 1px solid var(--soft);
  box-shadow: 0 6px 24px rgba(0,0,0,.24);
  font-family: system-ui, sans-serif; font-size: 13px;
}
#ann-bar.open { display: flex; }
#ann-bar button { font: inherit; cursor: pointer; border: 1px solid var(--soft); border-radius: 8px; background: transparent; color: var(--fg); padding: .28rem .55rem; }
#ann-bar .dot { width: 22px; height: 22px; padding: 0; border-radius: 50%; }
#ann-bar .dot.yellow { background: rgba(255,214,64,.8); }
#ann-bar .dot.green { background: rgba(126,217,87,.8); }
#ann-bar .dot.blue { background: rgba(94,168,255,.8); }
#ann-bar .dot.pink { background: rgba(255,143,195,.8); }
#ann-note {
  position: fixed; z-index: 46; display: none; width: min(88vw, 22rem);
  background: var(--bg); border: 1px solid var(--soft); border-radius: 12px;
  box-shadow: 0 8px 30px rgba(0,0,0,.26); padding: .6rem;
  font-family: system-ui, sans-serif; font-size: 14px;
}
#ann-note.open { display: block; }
#ann-note textarea {
  width: 100%; box-sizing: border-box; min-height: 5rem; resize: vertical;
  font: inherit; color: var(--fg); background: transparent;
  border: 1px solid var(--soft); border-radius: 8px; padding: .45rem;
}
#ann-note .row { display: flex; gap: .5rem; justify-content: flex-end; margin-top: .5rem; }
#ann-note button { font: inherit; padding: .32rem .75rem; border-radius: 8px; border: 1px solid var(--soft); background: transparent; color: var(--fg); cursor: pointer; }
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
  var defaults = { fs: 18, lh: 1.7, width: 40, margin: 1.25, font: 'serif', theme: 'day', align: 'left' };
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
    root.style.setProperty('--pad', settings.margin + 'rem');
    root.style.setProperty('--align', settings.align === 'justify' ? 'justify' : 'left');
    root.style.setProperty('--font', FONTS[settings.font] || FONTS.serif);
    body.setAttribute('data-theme', settings.theme);
    document.getElementById('s-fs').value = settings.fs;
    document.getElementById('s-lh').value = settings.lh;
    document.getElementById('s-width').value = settings.width;
    document.getElementById('s-margin').value = settings.margin;
    document.getElementById('v-fs').textContent = settings.fs + 'px';
    document.getElementById('v-lh').textContent = settings.lh.toFixed(2);
    document.getElementById('v-width').textContent = settings.width + 'rem';
    document.getElementById('v-margin').textContent = settings.margin + 'rem';
    markChips('c-font', 'font');
    markChips('c-theme', 'theme');
    markChips('c-align', 'align');
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
  document.getElementById('s-margin').addEventListener('input', function () { settings.margin = +this.value; apply(); });

  buildChips('c-font', [['serif', '宋体'], ['sans', '黑体'], ['kai', '楷体']], 'font');
  buildChips('c-theme', [['day', '日间'], ['sepia', '米黄'], ['green', '护眼'], ['night', '深灰'], ['black', '纯黑']], 'theme');
  buildChips('c-align', [['left', '左对齐'], ['justify', '两端对齐']], 'align');

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

  // ---- 批注（高亮/笔记）----
  var annBar = document.getElementById('ann-bar');
  var annNote = document.getElementById('ann-note');
  var annNoteText = annNote.querySelector('textarea');
  var pendingRange = null;
  var activeAnnId = null;
  var activeNoteAnnId = null;
  var COLORS = ['yellow', 'green', 'blue', 'pink'];

  function parentPost(message) { try { window.parent.postMessage(message, '*'); } catch (e) {} }
  function hideAnnBar() { annBar.classList.remove('open'); pendingRange = null; activeAnnId = null; }

  function blockOf(node) {
    var el = node && node.nodeType === 3 ? node.parentElement : node;
    return el && el.closest ? el.closest('[data-block]') : null;
  }
  function codePointLength(str) { return Array.from(str).length; }

  function selectionInfo() {
    var sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
    var range = sel.getRangeAt(0);
    var block = blockOf(range.startContainer);
    if (!block || block !== blockOf(range.endContainer)) return null;
    if (range.toString().trim() === '') return null;
    var existing = block.querySelectorAll('mark.hl');
    for (var i = 0; i < existing.length; i++) {
      if (range.intersectsNode(existing[i])) return null;
    }
    var pre = document.createRange();
    pre.selectNodeContents(block);
    try { pre.setEnd(range.startContainer, range.startOffset); } catch (e) { return null; }
    var start = codePointLength(pre.toString());
    return {
      blockId: block.dataset.block,
      start: start,
      end: start + codePointLength(range.toString()),
      text: range.toString(),
      range: range
    };
  }

  function positionBar(x, y) {
    annBar.classList.add('open');
    var width = annBar.offsetWidth || 240;
    var left = Math.min(Math.max(8, x - width / 2), window.innerWidth - width - 8);
    var top = y - annBar.offsetHeight - 10;
    if (top < 8) top = y + 20;
    annBar.style.left = left + 'px';
    annBar.style.top = top + 'px';
  }

  function buildBar(addMode) {
    annBar.innerHTML = '';
    COLORS.forEach(function (color) {
      var dot = document.createElement('button');
      dot.className = 'dot ' + color;
      dot.title = color;
      dot.addEventListener('click', function (event) {
        event.preventDefault();
        if (addMode && pendingRange) {
          parentPost({
            type: 'cadmus:addAnnotation',
            blockId: pendingRange.blockId,
            start: pendingRange.start,
            end: pendingRange.end,
            text: pendingRange.text,
            color: color
          });
          annBar.classList.remove('open');
        } else if (activeAnnId) {
          parentPost({ type: 'cadmus:annotationUpdated', id: activeAnnId, color: color });
          var mark = document.querySelector('mark[data-ann="' + activeAnnId + '"]');
          if (mark) mark.className = 'hl hl-' + color + (mark.dataset.note ? ' has-note' : '');
          hideAnnBar();
        }
      });
      annBar.appendChild(dot);
    });
    if (addMode) return;
    var noteBtn = document.createElement('button');
    noteBtn.textContent = '笔记';
    noteBtn.addEventListener('click', function (event) {
      event.preventDefault();
      if (!activeAnnId) return;
      activeNoteAnnId = activeAnnId;
      var mark = document.querySelector('mark[data-ann="' + activeAnnId + '"]');
      annNoteText.value = mark && mark.dataset.note ? mark.dataset.note : '';
      var rect = annBar.getBoundingClientRect();
      annNote.classList.add('open');
      var width = annNote.offsetWidth || 320;
      annNote.style.left = Math.min(Math.max(8, rect.left), window.innerWidth - width - 8) + 'px';
      annNote.style.top = Math.min(rect.bottom + 8, window.innerHeight - 230) + 'px';
      annNoteText.focus();
    });
    annBar.appendChild(noteBtn);
    var delBtn = document.createElement('button');
    delBtn.textContent = '删除';
    delBtn.addEventListener('click', function (event) {
      event.preventDefault();
      if (!activeAnnId) return;
      parentPost({ type: 'cadmus:annotationDeleted', id: activeAnnId });
      var mark = document.querySelector('mark[data-ann="' + activeAnnId + '"]');
      if (mark) {
        var parent = mark.parentNode;
        while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
        parent.removeChild(mark);
        parent.normalize();
      }
      hideAnnBar();
    });
    annBar.appendChild(delBtn);
  }

  var selTimer = null;
  document.addEventListener('selectionchange', function () {
    if (selTimer) clearTimeout(selTimer);
    selTimer = setTimeout(function () {
      if (annNote.classList.contains('open')) return;
      var info = selectionInfo();
      if (info) {
        pendingRange = info;
        buildBar(true);
        var rect = info.range.getBoundingClientRect();
        positionBar(rect.left + rect.width / 2, rect.top);
      } else if (!activeAnnId) {
        annBar.classList.remove('open');
      }
    }, 200);
  });

  document.addEventListener('click', function (event) {
    var mark = event.target && event.target.closest ? event.target.closest('mark.hl') : null;
    if (mark) {
      var sel = window.getSelection();
      if (sel) sel.removeAllRanges();
      activeAnnId = mark.dataset.ann;
      buildBar(false);
      var rect = mark.getBoundingClientRect();
      positionBar(rect.left + rect.width / 2, rect.top);
      return;
    }
    if (event.target && event.target.closest && event.target.closest('#ann-bar, #ann-note')) return;
    if (!annNote.classList.contains('open')) hideAnnBar();
  });

  window.addEventListener('scroll', function () { annBar.classList.remove('open'); }, { passive: true });

  document.getElementById('ann-note-cancel').addEventListener('click', function () {
    annNote.classList.remove('open');
  });  document.getElementById('ann-note-save').addEventListener('click', function () {
    if (activeNoteAnnId) {
      var value = annNoteText.value;
      parentPost({ type: 'cadmus:annotationUpdated', id: activeNoteAnnId, note: value });
      var mark = document.querySelector('mark[data-ann="' + activeNoteAnnId + '"]');
      if (mark) {
        if (value.trim()) {
          mark.dataset.note = value;
          mark.classList.add('has-note');
        } else {
          delete mark.dataset.note;
          mark.classList.remove('has-note');
        }
      }
    }
    activeNoteAnnId = null;
    annNote.classList.remove('open');
    annBar.classList.remove('open');
  });

  function currentAnchor() {
    var blocks = document.querySelectorAll('[data-block]');
    var best = null;
    for (var i = 0; i < blocks.length; i++) {
      if (blocks[i].getBoundingClientRect().bottom > 90) { best = blocks[i]; break; }
    }
    if (!best && blocks.length) best = blocks[blocks.length - 1];
    return best;
  }
  document.getElementById('bm-btn').addEventListener('click', function () {
    var anchor = currentAnchor();
    parentPost({
      type: 'cadmus:addBookmark',
      blockId: anchor ? anchor.dataset.block : null,
      label: anchor ? anchor.textContent.trim().slice(0, 30) : ''
    });
  });

  window.addEventListener('message', function (event) {
    var data = event.data || {};
    if (data.type === 'cadmus:annotationSaved') {
      if (pendingRange && pendingRange.range) {
        var mark = document.createElement('mark');
        mark.className = 'hl hl-' + (data.color || 'yellow');
        mark.dataset.ann = data.id;
        try {
          mark.appendChild(pendingRange.range.extractContents());
          pendingRange.range.insertNode(mark);
          var sel = window.getSelection();
          if (sel) sel.removeAllRanges();
        } catch (e) {}
      }
      pendingRange = null;
    } else if (data.type === 'cadmus:removeMark') {
      var stale = document.querySelector('mark[data-ann="' + data.id + '"]');
      if (stale) {
        var holder = stale.parentNode;
        while (stale.firstChild) holder.insertBefore(stale.firstChild, stale);
        holder.removeChild(stale);
        holder.normalize();
      }
    } else if (data.type === 'cadmus:jump') {
      var target = data.annId ? document.querySelector('mark[data-ann="' + data.annId + '"]') : null;
      if (!target && data.blockId) target = document.querySelector('[data-block="' + data.blockId + '"]');
      if (target) {
        target.scrollIntoView({ block: 'center', behavior: 'auto' });
        target.classList.remove('flash');
        void target.offsetWidth;
        target.classList.add('flash');
      }
    }
  });
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
    html.push_str("<button id=\"bm-btn\">书签</button>\n");
    html.push_str("<button id=\"set-btn\">Aa 设置</button>\n");
    html.push_str("</div>\n");

    html.push_str("<div id=\"scrim\"></div>\n");
    html.push_str("<aside id=\"toc\"><nav id=\"toc-nav\"></nav></aside>\n");
    html.push_str(
        "<div id=\"ann-bar\"></div>\n<div id=\"ann-note\"><textarea placeholder=\"写笔记…\"></textarea><div class=\"row\"><button id=\"ann-note-cancel\">取消</button><button id=\"ann-note-save\">保存</button></div></div>\n",
    );
    html.push_str("<section id=\"sheet\">\n<h3>阅读设置</h3>\n");
    html.push_str("<div class=\"srow\"><label>字号</label><input id=\"s-fs\" type=\"range\" min=\"13\" max=\"30\" step=\"1\"><span class=\"val\" id=\"v-fs\"></span></div>\n");
    html.push_str("<div class=\"srow\"><label>行距</label><input id=\"s-lh\" type=\"range\" min=\"1.3\" max=\"2.2\" step=\"0.05\"><span class=\"val\" id=\"v-lh\"></span></div>\n");
    html.push_str("<div class=\"srow\"><label>行宽</label><input id=\"s-width\" type=\"range\" min=\"26\" max=\"56\" step=\"1\"><span class=\"val\" id=\"v-width\"></span></div>\n");
    html.push_str("<div class=\"srow\"><label>边距</label><input id=\"s-margin\" type=\"range\" min=\"0.5\" max=\"4\" step=\"0.25\"><span class=\"val\" id=\"v-margin\"></span></div>\n");
    html.push_str(
        "<div class=\"srow\"><label>对齐</label><div class=\"chips\" id=\"c-align\"></div></div>\n",
    );
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
            let _ = writeln!(
                html,
                "<{tag} data-block=\"{}\">{}</{tag}>",
                block.id,
                marked_text(text, block.id, meta.marks)
            );
        }
        (BlockKind::Code, Payload::Text(text)) => {
            let _ = writeln!(
                html,
                "<pre data-block=\"{}\">{}</pre>",
                block.id,
                marked_text(text, block.id, meta.marks)
            );
        }
        (BlockKind::Footnote, Payload::Text(text)) => {
            let _ = writeln!(
                html,
                "<aside class=\"footnote\" data-block=\"{}\">{}</aside>",
                block.id,
                marked_text(text, block.id, meta.marks)
            );
        }
        (_, Payload::Text(text)) => {
            let page_attr = match block.page_range {
                Some((start, _)) => format!(" data-page=\"{start}\""),
                None => String::new(),
            };
            let _ = writeln!(
                html,
                "<p data-block=\"{}\"{}>{}</p>",
                block.id,
                page_attr,
                marked_text(text, block.id, meta.marks)
            );
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

/// 渲染块内文本并套用批注高亮（按字符偏移；重叠标记跳过，避免嵌套错误）。
fn marked_text(text: &str, block_id: u64, marks: &[Mark]) -> String {
    let block_key = block_id.to_string();
    let mut applicable: Vec<&Mark> = marks
        .iter()
        .filter(|mark| mark.block_id == block_key)
        .collect();
    if applicable.is_empty() {
        return escape(text);
    }
    applicable.sort_by_key(|mark| mark.start);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for mark in applicable {
        let start = (mark.start as usize).min(chars.len());
        let end = (mark.end as usize).min(chars.len());
        if start < cursor || end <= start {
            continue;
        }
        let before: String = chars[cursor..start].iter().collect();
        out.push_str(&escape(&before));
        let inner: String = chars[start..end].iter().collect();
        let note_attr = match &mark.note {
            Some(note) if !note.is_empty() => format!(" data-note=\"{}\"", escape(note)),
            _ => String::new(),
        };
        let note_class = if mark.note.is_some() { " has-note" } else { "" };
        let _ = write!(
            out,
            "<mark class=\"hl hl-{}{}\" data-ann=\"{}\"{}>{}</mark>",
            escape(&mark.color),
            note_class,
            escape(&mark.id),
            note_attr,
            escape(&inner)
        );
        cursor = end;
    }
    let rest: String = chars[cursor..].iter().collect();
    out.push_str(&escape(&rest));
    out
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
        .replace('"', "&quot;")
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
            marks: &[],
        }
    }

    #[test]
    fn renders_marks_around_ranges() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block::text(42, BlockKind::Paragraph, 0, "开头中间结尾")],
        };
        let marks = [Mark {
            block_id: "42".into(),
            start: 2,
            end: 4,
            id: "a1".into(),
            color: "yellow".into(),
            note: None,
        }];
        let meta = PreviewMeta {
            marks: &marks,
            ..base_meta("t")
        };
        let html = render_to_string(&document, Path::new(""), &meta).unwrap();
        assert!(
            html.contains(
                "<p data-block=\"42\">开头<mark class=\"hl hl-yellow\" data-ann=\"a1\">中间</mark>结尾</p>"
            ),
            "高亮应按字符区间包裹"
        );
    }

    #[test]
    fn marks_with_note_mark_the_marker() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block::text(5, BlockKind::Paragraph, 0, "句子")],
        };
        let marks = [Mark {
            block_id: "5".into(),
            start: 0,
            end: 2,
            id: "n1".into(),
            color: "blue".into(),
            note: Some("注解".into()),
        }];
        let meta = PreviewMeta {
            marks: &marks,
            ..base_meta("t")
        };
        let html = render_to_string(&document, Path::new(""), &meta).unwrap();
        assert!(
            html.contains("class=\"hl hl-blue has-note\""),
            "有笔记的高亮应带标记类"
        );
        assert!(html.contains("data-note=\"注解\""), "笔记内容应可预填");
    }

    #[test]
    fn overlapping_marks_are_skipped() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block::text(7, BlockKind::Paragraph, 0, "abcdef")],
        };
        let marks = [
            Mark {
                block_id: "7".into(),
                start: 0,
                end: 3,
                id: "m1".into(),
                color: "green".into(),
                note: None,
            },
            Mark {
                block_id: "7".into(),
                start: 2,
                end: 5,
                id: "m2".into(),
                color: "blue".into(),
                note: None,
            },
        ];
        let meta = PreviewMeta {
            marks: &marks,
            ..base_meta("t")
        };
        let html = render_to_string(&document, Path::new(""), &meta).unwrap();
        assert!(
            html.contains("<mark class=\"hl hl-green\" data-ann=\"m1\">abc</mark>"),
            "首个高亮保留"
        );
        assert!(!html.contains("data-ann=\"m2\""), "重叠的第二个高亮跳过");
        assert!(html.contains("def"), "余下文本完整");
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
            html.contains("<aside class=\"footnote\" data-block=\"3\">脚注正文：解释。</aside>"),
            "脚注块应渲染为 aside.footnote（带块锚点）"
        );
    }

    #[test]
    fn pdf_blocks_carry_page_anchor() {
        let document = Document {
            doc_hash: 1,
            title: None,
            blocks: vec![Block::text(9, BlockKind::Paragraph, 0, "正文").with_page_range((7, 7))],
        };
        let html = render_to_string(&document, Path::new(""), &base_meta("t")).unwrap();
        assert!(html.contains("data-page=\"7\""), "PDF 块应带原页锚点");
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
        assert!(html.contains("边距"), "缺边距滑条");
        assert!(html.contains("两端对齐"), "缺对齐选项");
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
