//! 错误类型（DESIGN-ENGINE §2.3）。

use std::fmt;

/// crate 级 `Result`。
pub type Result<T> = std::result::Result<T, Error>;

/// cadmus 引擎错误。
#[derive(Debug)]
pub enum Error {
    /// 文件读写失败。
    Io(std::io::Error),
    /// 加密文档；M1 不支持打开。
    Encrypted,
    /// 格式不受支持或数据损坏。
    Unsupported(String),
    /// 解析失败（单页/单章级降级由调用方处理，此处为整体失败）。
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO 错误：{e}"),
            Self::Encrypted => write!(f, "文档已加密，M1 不支持打开"),
            Self::Unsupported(msg) => write!(f, "不支持：{msg}"),
            Self::Parse(msg) => write!(f, "解析失败：{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages() {
        assert_eq!(Error::Encrypted.to_string(), "文档已加密，M1 不支持打开");
        assert!(
            Error::Unsupported("mobi".into())
                .to_string()
                .contains("mobi")
        );
    }
}
