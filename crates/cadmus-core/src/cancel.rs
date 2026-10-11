//! 取消令牌（DESIGN-ENGINE §2.4）。
//!
//! 引擎 API 接受 `Cancel`，取消后立即返回，不落半成品。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 可克隆的取消令牌。
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求取消。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// 是否已请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
