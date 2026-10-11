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
                push_text(out, BlockKind::Paragraph, collect_text(child));
            }
            "pre" => push_text(out, BlockKind::Code, collect_text(child)),
            "table" => push_text(out, BlockKind::Table, collect_table(child)),
            "img" => {
                if let Some(src) = child.attribute("src") {
                    out.push(ContentBlock::Image {
                        src: src.to_string(),
                    });
                }
            }
            _ => walk(child, out),
        }
    }
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
