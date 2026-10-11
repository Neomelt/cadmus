//! EPUB 容器解析：META-INF/container.xml → OPF 路径（DESIGN-ENGINE §4.3）。

use cadmus_core::{Error, Result};

/// 从 container.xml 取第一个 rootfile 的 full-path。
pub fn rootfile_path(xml: &str) -> Result<String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| Error::Parse(format!("container.xml 解析失败：{error}")))?;
    let rootfile = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .ok_or_else(|| Error::Parse("container.xml 缺少 rootfile".into()))?;
    rootfile
        .attribute("full-path")
        .map(str::to_string)
        .ok_or_else(|| Error::Parse("rootfile 缺少 full-path".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rootfile() {
        let xml = r#"<?xml version="1.0"?>
        <container version="1.0">
          <rootfiles>
            <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
          </rootfiles>
        </container>"#;
        assert_eq!(rootfile_path(xml).unwrap(), "OEBPS/content.opf");
    }
}
