//! kmoefix 单文件入口：同一个 exe 既是命令行工具，也是图形界面。
//!
//! 分发规则见 [`kmoefix::cli`]：不带参数（双击）打开 GUI，带文件走命令行批处理，
//! `--gui` 显式开 GUI。release 版以 GUI 子系统编译，双击不弹控制台窗口；命令行模式
//! 下由 [`kmoefix::console`] 借用或新建控制台，终端里照常有输出。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::Path;

use kmoefix::{
    console, fix_one_with, parse_command_line, print_line, print_usage, Command, FixOptions,
    KmoeError, RotateCover, VERSION,
};

fn main() {
    let cmd = match parse_command_line(std::env::args().skip(1).collect()) {
        Ok(cmd) => cmd,
        Err(msg) => {
            console::attach_for_cli();
            print_line(&msg);
            print_usage();
            std::process::exit(2);
        }
    };

    match cmd {
        Command::Gui { preload } => {
            if let Err(e) = kmoefix::run_gui(preload) {
                // 双击启动时没有终端可以承载错误信息，用弹窗兜底
                let _ = rfd::MessageDialog::new()
                    .set_title("KmoeFix")
                    .set_description(format!("图形界面启动失败: {e}"))
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
                std::process::exit(1);
            }
        }
        Command::Usage => {
            console::attach_for_cli();
            print_usage();
        }
        Command::Version => {
            console::attach_for_cli();
            print_line(&format!("kmoefix {VERSION}"));
        }
        Command::Cli { opts, files } => {
            console::attach_for_cli();
            run_cli(opts, files);
        }
    }
}

/// 逐个处理存在的文件，`[OK]`/`[FAIL]` 前缀 + 路径。
fn run_cli(opts: FixOptions, files: Vec<String>) {
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
