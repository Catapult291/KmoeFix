//! kmoefix CLI：用法与 Python `python -m src.kmoe_fix` 一致。
//!
//! - 无参数：提示（Rust 版暂未内置 GUI）
//! - 带参数：逐个处理存在的文件，[OK]/[FAIL] 前缀 + 路径

use kmoefix::{fix_one, KmoeError};
use std::io::Write;
use std::path::Path;

fn print_line(s: &str) {
    // Windows 控制台可能是 GBK 代码页（chcp 936）。输出先尝试按 UTF-8 写，
    // 失败则退化为「替换不可编码字节」，避免像 Python 直接 print 那样抛
    // UnicodeEncodeError 中断批量处理。
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    match writeln!(lock, "{s}") {
        Ok(_) => {}
        Err(_) => {
            // 编码失败：把非 ASCII 字节替换掉再输出（与 [OK]/[FAIL] 前缀共同
            // 保证机器可读；路径本身仍可被文件系统正确处理）
            let ascii_safe: String = s
                .chars()
                .map(|c| if c.is_ascii() { c } else { '\u{fffd}' })
                .collect();
            let _ = writeln!(lock, "{ascii_safe}");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        print_line("kmoefix - 修复 Kmoe 导出 EPUB（Rust 移植版）");
        print_line("用法: kmoefix <file.epub> [file2.epub ...]");
        print_line("（原 Python 版无参数时启动 GUI；Rust 版 GUI 尚未实现）");
        std::process::exit(0);
    }

    let mut any_fail = false;
    for arg in &args {
        let p = Path::new(arg);
        if !p.is_file() {
            print_line(&format!("跳过不存在: {arg}"));
            continue;
        }
        match fix_one(arg, None, None) {
            Ok(out) => print_line(&format!("[OK] {arg} -> {}", out.dst.display())),
            Err(KmoeError { msg }) => {
                any_fail = true;
                print_line(&format!("[FAIL] {arg}: {msg}"));
            }
        }
    }
    if any_fail {
        std::process::exit(1);
    }
}
