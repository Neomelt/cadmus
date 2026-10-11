//! cadmus 内容解析：TXT 与 EPUB（DESIGN-ENGINE §4.3、§4.4）。

mod chapters;
mod container;
mod content;
mod encoding;
mod epub;
mod opf;
pub mod txt;

pub use encoding::DecodeReport;
pub use epub::{EpubOptions, EpubReport, parse_epub, parse_epub_bytes};
pub use txt::{TxtOptions, parse_txt, parse_txt_bytes};
