//! OPF 解析：metadata / manifest / spine（DESIGN-ENGINE §4.3）。

use cadmus_core::{Error, Result};

#[derive(Debug, Clone)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct Opf {
    pub title: Option<String>,
    pub language: Option<String>,
    pub manifest: Vec<ManifestItem>,
    /// spine 中 idref 的顺序即阅读顺序（linear="no" 的条目已排除）。
    pub spine: Vec<String>,
}

pub fn parse(xml: &str) -> Result<Opf> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| Error::Parse(format!("OPF 解析失败：{error}")))?;

    let title = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "title")
        .and_then(|node| node.text())
        .map(str::to_string);
    let language = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "language")
        .and_then(|node| node.text())
        .map(str::to_string);

    let mut manifest = Vec::new();
    for item in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "item")
    {
        let (Some(id), Some(href)) = (item.attribute("id"), item.attribute("href")) else {
            continue;
        };
        manifest.push(ManifestItem {
            id: id.to_string(),
            href: href.to_string(),
            media_type: item.attribute("media-type").unwrap_or("").to_string(),
        });
    }

    let mut spine = Vec::new();
    for itemref in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "itemref")
    {
        if itemref.attribute("linear") == Some("no") {
            continue;
        }
        if let Some(idref) = itemref.attribute("idref") {
            spine.push(idref.to_string());
        }
    }

    Ok(Opf {
        title,
        language,
        manifest,
        spine,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_spine() {
        let xml = r#"<?xml version="1.0"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:title>测试书</dc:title>
            <dc:language>zh</dc:language>
          </metadata>
          <manifest>
            <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            <item id="css" href="style.css" media-type="text/css"/>
          </manifest>
          <spine>
            <itemref idref="ch1"/>
            <itemref idref="css" linear="no"/>
          </spine>
        </package>"#;
        let opf = parse(xml).unwrap();
        assert_eq!(opf.title.as_deref(), Some("测试书"));
        assert_eq!(opf.language.as_deref(), Some("zh"));
        assert_eq!(opf.manifest.len(), 2);
        assert_eq!(opf.spine, vec!["ch1"]);
    }
}
