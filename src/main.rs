//! kmoefix CLI：用法与 Python `python -m src.kmoe_fix` 一致，另加侧放回正开关。
//!
//! - 无参数 / `--help`：用法提示
//! - 带参数：逐个处理存在的文件，[OK]/[FAIL] 前缀 + 路径
//! - `--no-rotate-cover` / `--rotate-cover[=auto|90|180|270|off]`：侧放页回正
//!   （详见 `cover.rs`）。**默认开启**，只认包内参照图 `<名字>-RAWIMAGE.<ext>`；
//!   `--no-rotate-cover` 时图片一个像素都不动

use kmoefix::{fix_one_with, FixOptions, KmoeError, RotateCover};
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

fn print_usage() {
    print_line("kmoefix - 修复 Kmoe 导出 EPUB（Rust 移植版）");
    print_line("用法: kmoefix [--no-rotate-cover] [--rotate-cover[=auto|90|180|270|off]] <file.epub> ...");
    print_line("");
    print_line("修复（默认路径）:");
    print_line("  --no-rotate-cover     不做任何图片处理（图片与原版逐字节一致；站点卡片页的移位照常）");
    print_line("  --rotate-cover=90     判出侧放后按顺时针 90 度回正（覆盖自动判定的方向；180/270 同理）");
    print_line("  不带开关时 = 自动回正：只有包内有同名参照图 <图名>-RAWIMAGE.<ext> 时才转，");
    print_line("  参照图缺失或相似度不足一律不动，不做尺寸猜测");
}

/// 解析修复路径的参数：返回（选项, 文件列表, 是否要打印用法）。
fn parse_args(args: Vec<String>) -> Result<(FixOptions, Vec<String>, bool), String> {
    let mut opts = FixOptions::default();
    let mut files = Vec::new();
    let mut usage = false;
    for arg in args {
        if arg == "--help" || arg == "-h" {
            usage = true;
            continue;
        }
        if arg == "--no-rotate-cover" {
            opts.rotate_cover = RotateCover::Off;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--rotate-cover") {
            let value = value.strip_prefix('=').unwrap_or("auto");
            opts.rotate_cover = match value {
                "auto" => RotateCover::Auto,
                "off" => RotateCover::Off,
                "90" => RotateCover::Fixed(90),
                "180" => RotateCover::Fixed(180),
                "270" => RotateCover::Fixed(270),
                other => return Err(format!("--rotate-cover 取值无效: {other}")),
            };
            continue;
        }
        files.push(arg);
    }
    Ok((opts, files, usage))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let (opts, files, usage) = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(msg) => {
            print_line(&msg);
            print_usage();
            std::process::exit(2);
        }
    };

    if usage || files.is_empty() {
        print_usage();
        if files.is_empty() && !usage {
            // 与原版一致：无参数时给出用法提示并正常退出（原 Python 版此处启动 GUI）
            print_line("（原 Python 版无参数时启动 GUI；Rust 版请使用 kmoefix_gui）");
        }
        std::process::exit(0);
    }

    // 关掉回正时保持与原版一致的安静输出；其余情况透出核心的处理日志
    let verbose = opts.rotate_cover != RotateCover::Off;
    let log_fn = |s: &str| print_line(s);
    let log: Option<&dyn Fn(&str)> = if verbose { Some(&log_fn) } else { None };

    let mut any_fail = false;
    for arg in &files {
        let p = Path::new(arg);
        if !p.is_file() {
            print_line(&format!("跳过不存在: {arg}"));
            continue;
        }
        match fix_one_with(arg, None, log, &opts) {
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
