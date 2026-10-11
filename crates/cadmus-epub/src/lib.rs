//! cadmus 内容解析：TXT 与 EPUB（DESIGN-ENGINE §4.3、§4.4）。
//!
//! M1 阶段先实现 TXT 管线；EPUB 解析随后补齐。

mod chapters;
mod encoding;
pub mod txt;

pub use encoding::DecodeReport;
pub use txt::{TxtOptions, parse_txt, parse_txt_bytes};
