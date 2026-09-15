//! KmoeFix — Rust 实现的漫画 EPUB 页序与文件名修复工具。
//!
//! 核心流程：`fix_one` 按页面标题解析真实话数，重命名 html/image 条目、重排 spine、
//! 改写 opf 与 nav 引用、清除脏属性；写盘前回读校验页序必须连续 `1..N`。
//!
//! 三项默认开启的能力（细节见 `docs/`）：
//! - spine 乱序输入按话数重排修复，详见 `core.rs`「按话数排序」注释；
//! - 侧放页回正（按包内参照图判定方向，覆盖封面、正文第 1 页与任何带参照图的页），
//!   详见 `cover.rs`；`--no-rotate-cover` 可整体关闭；
//! - 站点卡片页移出正文编号、排到卷末，详见 `core.rs`「站点卡片页」注释。
//!
//! 改动前必读：
//! - zip 条目名一律按 **原始字节** 存取（epub 里是 UTF-8）；`zip` crate 会自行解码
//!   非 ASCII 名字，必须用 `raw` 字段写回才能字节无损。
//! - 引用改写先一次扫描算出旧→新映射，再逐条替换一次，避免名字接力（`page-150`
//!   让位给 `page-151`）造成二次替换；替换不区分 `src="../x"` 与 `src="x"` 两种书写。
//! - 编号宽度 = `max(3, 最大话数位数)`，右对齐补零。
//! - 回读校验的 manifest 解析**只匹配新写入的 opf**，与输出前的全量解析是两套不同来源。

mod cli;
mod core;
mod cover;
mod gui;

pub mod console;

pub use cli::{parse_command_line, print_line, print_usage, Command};
pub use core::{
    fix_one, fix_one_with, get_unique_dst, FixOptions, FixOutcome, KmoeError, RotateCover,
};
pub use gui::run_gui;

/// 版本号：`Cargo.toml` 是唯一来源，release 标签、`--version` 与 GUI 标题都用它。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
