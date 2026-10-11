//! cadmus 核心：文档块模型、锚点与稳定标识。
//!
//! 本 crate 不依赖 UI 或平台，供 reflow / epub / cli 以及后续的 app 复用。
//! 对应设计见 `docs/DESIGN-ENGINE.md` §2.2、§4.1。

pub mod anchor;
pub mod cancel;
pub mod error;
pub mod hash;
pub mod model;

pub use anchor::{Anchor, BlockId, stable_block_id};
pub use cancel::Cancel;
pub use error::{Error, Result};
pub use hash::content_hash;
pub use model::{BitmapRef, Block, BlockKind, Document, PageRange, Payload};
