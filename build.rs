//! 把 `assets/kmoefix.ico` 编进 PE 资源，双击 exe 与资源管理器才显示项目图标。
//!
//! 用的是 `tauri-winres`：它生成 .rc 并调用 Windows SDK 的 `rc.exe` 编译成 COFF 资源
//! 对象，再由链接器并入 exe。非 Windows 平台跳过。

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/kmoefix.ico");
        tauri_winres::WindowsResource::new()
            .set_icon("assets/kmoefix.ico")
            // exe 属性面板里的文案；ProductName / 版本号由插件从 Cargo.toml 取
            .set("FileDescription", "漫画 EPUB 页序修复工具")
            .set("LegalCopyright", "Copyright (c) 2026 Catapult291")
            .compile()
            .expect("嵌入 exe 图标失败（MSVC 工具链需要 Windows SDK 的 rc.exe）");
    }
}
