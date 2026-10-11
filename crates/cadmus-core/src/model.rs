//! 文档块模型（DESIGN-ENGINE §2.2）。

use crate::anchor::BlockId;

/// 原页范围（含端点）。仅 PDF 使用；EPUB/TXT 为 `None`。
pub type PageRange = (u32, u32);

/// 块类型。
///
/// 非文本块（图/表/公式/代码/降级页）始终作为不可分割的原子参与阅读流。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// 标题；`level` 为层级（1 起）。
    Heading {
        level: u8,
    },
    Paragraph,
    Figure,
    Table,
    Formula,
    Code,
    Footnote,
    /// 整页降级位图（无文本层或布局分析失败）。
    PageBitmap,
}

/// 位图引用（原子块或降级页）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapRef {
    /// 位图文件路径（导出目录或缓存内）。
    pub path: String,
    pub width_px: u32,
    pub height_px: u32,
}

/// 块内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// 语义文本（段落/标题等），后续由 WebView 排版。
    Text(String),
    /// 位图（原子块或降级页）。
    Bitmap(BitmapRef),
}

/// 阅读流中的一个块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// 稳定标识，由 [`crate::stable_block_id`] 派生。
    pub id: BlockId,
    pub kind: BlockKind,
    /// 阅读顺序（0 起连续）。
    pub order: u32,
    /// PDF 的原页范围；EPUB/TXT 为 `None`。
    pub page_range: Option<PageRange>,
    pub payload: Payload,
}

impl Block {
    /// 构造文本块（`page_range` 默认 `None`）。
    pub fn text(id: BlockId, kind: BlockKind, order: u32, text: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            order,
            page_range: None,
            payload: Payload::Text(text.into()),
        }
    }

    /// 构造位图块（`page_range` 默认 `None`）。
    pub fn bitmap(id: BlockId, kind: BlockKind, order: u32, bitmap: BitmapRef) -> Self {
        Self {
            id,
            kind,
            order,
            page_range: None,
            payload: Payload::Bitmap(bitmap),
        }
    }

    /// 附加原页范围（仅 PDF）。
    #[must_use]
    pub fn with_page_range(mut self, range: PageRange) -> Self {
        self.page_range = Some(range);
        self
    }

    /// 文本块的文本；位图块返回 `None`。
    pub fn as_text(&self) -> Option<&str> {
        match &self.payload {
            Payload::Text(text) => Some(text),
            Payload::Bitmap(_) => None,
        }
    }
}

/// 一个文档的完整块序列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// 源文件内容哈希（[`crate::content_hash`]）。
    pub doc_hash: u64,
    pub title: Option<String>,
    pub blocks: Vec<Block>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_block_roundtrip() {
        let block = Block::text(7, BlockKind::Paragraph, 0, "第一段").with_page_range((2, 3));
        assert_eq!(block.as_text(), Some("第一段"));
        assert_eq!(block.page_range, Some((2, 3)));
    }

    #[test]
    fn bitmap_block_has_no_text() {
        let bitmap = BitmapRef {
            path: "cache/p3-f1.png".into(),
            width_px: 1200,
            height_px: 800,
        };
        let block = Block::bitmap(9, BlockKind::Figure, 1, bitmap.clone());
        assert_eq!(block.as_text(), None);
        assert_eq!(block.payload, Payload::Bitmap(bitmap));
    }
}
