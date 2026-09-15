//! 命令行解析与分发：CLI 与 GUI 是同一个 exe。
//!
//! - 不带参数（双击 exe）→ 图形界面
//! - `--gui [文件…]` → 图形界面，并把给定文件装填进列表
//! - `--help` / `-h`、`--version` / `-V` → 打印后退出
//! - 其余参数 → 命令行批处理（`--no-rotate-cover` / `--rotate-cover=…` + 文件）

use std::io::Write;
use std::path::PathBuf;

use crate::{FixOptions, RotateCover, VERSION};

/// 解析后的动作。
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// 打开图形界面；`preload` 是预装进列表的文件（可空）。
    Gui { preload: Vec<PathBuf> },
    /// 打印用法。
    Usage,
    /// 打印版本。
    Version,
    /// 命令行批处理。
    Cli { opts: FixOptions, files: Vec<String> },
}

/// 按参数决定走 GUI 还是 CLI。返回 `Err` 表示参数非法（调用方打印后以退出码 2 结束）。
pub fn parse_command_line(args: Vec<String>) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Gui { preload: Vec::new() });
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Command::Usage);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Command::Version);
    }

    let mut opts = FixOptions::default();
    let mut files = Vec::new();
    let mut gui = false;
    for arg in args {
        match arg.as_str() {
            "--gui" => {
                gui = true;
                continue;
            }
            "--no-rotate-cover" => {
                opts.rotate_cover = RotateCover::Off;
                continue;
            }
            _ => {}
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

    if gui {
        return Ok(Command::Gui { preload: files.into_iter().map(PathBuf::from).collect() });
    }
    if files.is_empty() {
        return Ok(Command::Usage);
    }
    Ok(Command::Cli { opts, files })
}

/// 输出一行。Windows 控制台可能是 GBK 代码页（chcp 936）。先尝试按 UTF-8 写，
/// 失败则退化为「替换不可编码字节」，不因编码错误中断批量处理。
pub fn print_line(s: &str) {
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

pub fn print_usage() {
    print_line(&format!("KmoeFix {VERSION} - 修复 Kmoe 导出 EPUB（CLI 与图形界面同一个程序）"));
    print_line("用法:");
    print_line("  KmoeFix                             不带参数：打开图形界面（双击 exe 同样）");
    print_line("  KmoeFix <file.epub> ...             命令行批量处理");
    print_line("  KmoeFix --gui [file.epub ...]       打开图形界面，并把文件装填进列表");
    print_line("  KmoeFix --help | --version");
    print_line("");
    print_line("选项（仅命令行批处理）:");
    print_line("  --no-rotate-cover     不做任何图片处理（图片逐字节原样写出；站点卡片页的移位照常）");
    print_line("  --rotate-cover=90     判出侧放后按顺时针 90 度回正（覆盖自动判定的方向；180/270 同理）");
    print_line("  不带开关时 = 自动回正：只有包内有同名参照图 <图名>-RAWIMAGE.<ext> 时才转，");
    print_line("  参照图缺失或相似度不足一律不动，不做尺寸猜测");
    print_line("");
    print_line("输出与退出码:");
    print_line("  产物写到源文件同目录的 *_修正版.epub（已存在则递增为 *_修正版 (1).epub），不覆盖原文件");
    print_line("  全部成功退出码 0；任一文件失败 1；参数非法 2");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_opens_gui() {
        // 双击（无参数）打开图形界面
        assert_eq!(parse_command_line(args(&[])), Ok(Command::Gui { preload: Vec::new() }));
    }

    #[test]
    fn gui_flag_preloads_files_and_dirs() {
        assert_eq!(
            parse_command_line(args(&["--gui", "a.epub", "b.zip"])),
            Ok(Command::Gui { preload: vec![PathBuf::from("a.epub"), PathBuf::from("b.zip")] })
        );
        assert_eq!(parse_command_line(args(&["--gui"])), Ok(Command::Gui { preload: Vec::new() }));
    }

    #[test]
    fn files_without_gui_stay_cli() {
        assert_eq!(
            parse_command_line(args(&["a.epub"])),
            Ok(Command::Cli { opts: FixOptions::default(), files: vec!["a.epub".to_string()] })
        );
    }

    #[test]
    fn rotate_flags_map_to_options() {
        assert_eq!(
            parse_command_line(args(&["--no-rotate-cover", "a.epub"])),
            Ok(Command::Cli {
                opts: FixOptions { rotate_cover: RotateCover::Off },
                files: vec!["a.epub".to_string()],
            })
        );
        assert_eq!(
            parse_command_line(args(&["--rotate-cover=270", "a.epub"])),
            Ok(Command::Cli {
                opts: FixOptions { rotate_cover: RotateCover::Fixed(270) },
                files: vec!["a.epub".to_string()],
            })
        );
        // 不带 = 值等同 auto
        assert_eq!(
            parse_command_line(args(&["--rotate-cover", "a.epub"])),
            Ok(Command::Cli {
                opts: FixOptions { rotate_cover: RotateCover::Auto },
                files: vec!["a.epub".to_string()],
            })
        );
        assert!(parse_command_line(args(&["--rotate-cover=45", "a.epub"])).is_err());
    }

    #[test]
    fn help_and_version_win_over_files() {
        assert_eq!(parse_command_line(args(&["a.epub", "--help"])), Ok(Command::Usage));
        assert_eq!(parse_command_line(args(&["-h"])), Ok(Command::Usage));
        assert_eq!(parse_command_line(args(&["a.epub", "--version"])), Ok(Command::Version));
        assert_eq!(parse_command_line(args(&["-V"])), Ok(Command::Version));
    }

    #[test]
    fn flags_without_files_print_usage() {
        assert_eq!(parse_command_line(args(&["--no-rotate-cover"])), Ok(Command::Usage));
    }
}
