//! XHTML → 块序列（DESIGN-ENGINE §4.3）。

use cadmus_core::{BlockKind, Error, Result};

/// XHTML 内容块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentBlock {
    Text {
        kind: BlockKind,
        text: String,
    },
    /// 图片：`src` 为相对章节文件的路径（调用方负责解析与提取）。
    Image {
        src: String,
    },
}

/// 解析一章 XHTML。
pub fn parse(xml: &str) -> Result<Vec<ContentBlock>> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| Error::Parse(format!("XHTML 解析失败：{error}")))?;
    let body = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "body");
    let root = body.unwrap_or_else(|| document.root_element());
    let mut blocks = Vec::new();
    walk(root, &mut blocks);
    Ok(blocks)
}

fn walk(node: roxmltree::Node<'_, '_>, out: &mut Vec<ContentBlock>) {
    for child in node.children().filter(|child| child.is_element()) {
        let tag = child.tag_name().name().to_ascii_lowercase();
        match tag.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag[1..].parse::<u8>().unwrap_or(1);
                push_text(out, BlockKind::Heading { level }, collect_text(child));
            }
            "p" | "blockquote" | "li" | "figcaption" => {
                flush_container(out, BlockKind::Paragraph, child);
            }
            "pre" => push_text(out, BlockKind::Code, collect_text(child)),
            "table" => push_text(out, BlockKind::Table, collect_table(child)),
            "img" | "svg" => push_image(out, child),
            _ => walk(child, out),
        }
    }
}

/// 段落类容器：文本与内嵌图片按出现顺序输出（`<p>文字<img/></p>` 不丢图）。
fn flush_container(out: &mut Vec<ContentBlock>, kind: BlockKind, node: roxmltree::Node<'_, '_>) {
    let mut pending = String::new();
    collect_mixed(node, out, &mut pending);
    push_text(out, kind, pending);
}

/// 收集文本与图片的混排：文本累积在 `pending`，遇到图片先冲出文本块再输出图片。
fn collect_mixed(node: roxmltree::Node<'_, '_>, out: &mut Vec<ContentBlock>, pending: &mut String) {
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                pending.push_str(text);
            }
        } else if child.is_element() {
            let tag = child.tag_name().name().to_ascii_lowercase();
            match tag.as_str() {
                "img" | "svg" => {
                    push_text(out, BlockKind::Paragraph, std::mem::take(pending));
                    push_image(out, child);
                }
                "br" => pending.push('\n'),
                _ => collect_mixed(child, out, pending),
            }
        }
    }
}

/// 输出图片块：`<img src>` 或 SVG `<image href>`。
fn push_image(out: &mut Vec<ContentBlock>, node: roxmltree::Node<'_, '_>) {
    let src = if node.tag_name().name().eq_ignore_ascii_case("svg") {
        svg_image_href(node)
    } else {
        node.attribute("src").map(str::to_string)
    };
    if let Some(src) = src {
        out.push(ContentBlock::Image { src });
    }
}

/// `<svg>` 内 `<image>` 引用的位图（href 或 xlink:href）。
fn svg_image_href(node: roxmltree::Node<'_, '_>) -> Option<String> {
    node.descendants()
        .filter(|descendant| descendant.is_element())
        .filter(|descendant| descendant.tag_name().name().eq_ignore_ascii_case("image"))
        .find_map(|image| {
            image
                .attribute("href")
                .or_else(|| image.attribute(("http://www.w3.org/1999/xlink", "href")))
                .map(str::to_string)
        })
}

fn push_text(out: &mut Vec<ContentBlock>, kind: BlockKind, text: String) {
    if text.trim().is_empty() {
        return;
    }
    out.push(ContentBlock::Text {
        kind,
        text: text.trim().to_string(),
    });
}

fn collect_text(node: roxmltree::Node<'_, '_>) -> String {
    node.descendants()
        .filter(|descendant| descendant.is_text())
        .filter_map(|descendant| descendant.text())
        .collect::<String>()
}

fn collect_table(node: roxmltree::Node<'_, '_>) -> String {
    let mut rows = Vec::new();
    for row in node
        .descendants()
        .filter(|descendant| descendant.is_element() && descendant.tag_name().name() == "tr")
    {
        let cells: Vec<String> = row
            .children()
            .filter(|cell| cell.is_element() && matches!(cell.tag_name().name(), "td" | "th"))
            .map(collect_text)
            .map(|text| text.trim().to_string())
            .collect();
        if !cells.is_empty() {
            rows.push(cells.join(" | "));
        }
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_images_inside_paragraphs() {
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink"><body>
            <p>前图：<img src="a.png"/>后文。</p>
            <li>条目 <img src="b.png"/></li>
            <p><svg viewBox="0 0 10 10"><image xlink:href="c.jpg" width="10" height="10"/></svg></p>
        </body></html>"#;
        let blocks = parse(xml).unwrap();
        assert_eq!(
            blocks[0],
            ContentBlock::Text {
                kind: BlockKind::Paragraph,
                text: "前图：".into()
            },
            "图片前的文本应保留"
        );
        assert_eq!(
            blocks[1],
            ContentBlock::Image {
                src: "a.png".into()
            },
            "段落内图片不能丢，且顺序不变"
        );
        assert_eq!(
            blocks[2],
            ContentBlock::Text {
                kind: BlockKind::Paragraph,
                text: "后文。".into()
            },
            "图片后的文本应保留"
        );
        assert!(
            blocks.contains(&ContentBlock::Image {
                src: "b.png".into()
            }),
            "列表项内图片不能丢"
        );
        assert!(
            blocks.contains(&ContentBlock::Image {
                src: "c.jpg".into()
            }),
            "SVG <image> 图片不能丢"
        );
    }

    #[test]
    fn converts_xhtml() {
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <h2>第一节</h2><p>你好。</p><img src="a.png"/>
            <table><tr><td>A</td><td>B</td></tr><tr><td>1</td><td>2</td></tr></table>
            <pre>let x = 1;</pre>
        </body></html>"#;
        let blocks = parse(xml).unwrap();
        assert_eq!(
            blocks[0],
            ContentBlock::Text {
                kind: BlockKind::Heading { level: 2 },
                text: "第一节".into()
            }
        );
        assert_eq!(
            blocks[2],
            ContentBlock::Image {
                src: "a.png".into()
            }
        );
        assert!(matches!(
            blocks[3],
            ContentBlock::Text {
                kind: BlockKind::Table,
                ..
            }
        ));
        assert!(matches!(
            blocks[4],
            ContentBlock::Text {
                kind: BlockKind::Code,
                ..
            }
        ));
    }
}
