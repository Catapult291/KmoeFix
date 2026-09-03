//! kmoefix — Rust 实现的 Kmoe 漫画 EPUB 修复工具。
//!
//! 核心语义对齐自 Python 原版 `src/core.py`（fix_one / get_unique_dst），
//! 对拍基准是原仓库 `tests/test_fix_one.py` 展开后的 12 个场景。
//! 与原版唯一的行为差异是能力扩展：spine 乱序输入会被按话数重排修复
//! （原版一律回滚），详见 `core.rs`「按话数排序」注释。
//! 注意事项（移植时踩过的 Python 语义，改动前必读）：
//! - zip 文件名一律按 **原始字节** 存取（epub 里是 UTF-8），Python `zipfile` 会给含非 ASCII
//!   的名字置通用位 11；Rust 侧 `zip` crate 会自行解码，必须用 `raw` 字段写回才能字节无损。
//! - `str.replace` 是**全量替换**（无 `g` 标志的 JS 是只换第一个，别混淆），且替换不区分
//!   `src="../x"` / `src="x"` 两种书写。
//! - 数值格式 `0{width}d` 是右对齐补零；`max(3, ...)` 兜底保证宽度至少 3。
//! - 回读校验的 manifest 解析**只匹配新写入的 opf**，与输出前的全量解析是两套不同来源。

mod core;

pub use core::{fix_one, get_unique_dst, FixOutcome, KmoeError};

/// 暴露给测试的小工具（不构成公共 API）。
#[doc(hidden)]
pub mod test_helpers {
    use regex::Regex;
    /// 从 opf 文本里按顺序抽取 spine 的 idref（与 SPINE_RE 一致）。
    pub fn spine_idrefs(opf: &str) -> Vec<String> {
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r#"<itemref[^>]*idref="([^"]+)"[^>]*>"#).unwrap());
        re.captures_iter(opf).map(|c| c[1].to_string()).collect()
    }
}

#[cfg(test)]
mod tests;
