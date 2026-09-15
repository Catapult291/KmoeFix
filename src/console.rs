//! Windows 控制台接管。
//!
//! exe 以 GUI 子系统编译（双击时不弹控制台窗口），因此走命令行时必须自己拿到一个
//! 控制台：先从父进程借（cmd / PowerShell / ConPTY 终端），借不到再新建。接管在做
//! 第一次输出之前完成，之后标准库的输出直接落到终端；GUI 模式不调用这里，所以双击
//! 不会出现终端窗口，也不占用任务栏。

/// 命令行模式使用：确保 stdout/stderr 有去处。其他平台无事可做。
#[cfg(windows)]
pub fn attach_for_cli() {
    imp::attach()
}

#[cfg(not(windows))]
pub fn attach_for_cli() {}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    type Handle = *mut c_void;

    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const FILE_TYPE_DISK: u32 = 0x0001;
    const FILE_TYPE_CHAR: u32 = 0x0002;
    const FILE_TYPE_PIPE: u32 = 0x0003;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;

    extern "system" {
        fn GetStdHandle(n_std_handle: u32) -> Handle;
        fn SetStdHandle(n_std_handle: u32, h_handle: Handle) -> i32;
        fn GetFileType(h_file: Handle) -> u32;
        fn GetConsoleMode(h_console_handle: Handle, lp_mode: *mut u32) -> i32;
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn AllocConsole() -> i32;
        fn CreateFileW(
            lp_file_name: *const u16,
            dw_desired_access: u32,
            dw_share_mode: u32,
            lp_security_attributes: *mut c_void,
            dw_creation_disposition: u32,
            dw_flags_and_attributes: u32,
            h_template_file: Handle,
        ) -> Handle;
    }

    fn open_console(name: &str) -> Handle {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        }
    }

    fn valid(h: Handle) -> bool {
        !h.is_null() && h != INVALID_HANDLE_VALUE
    }

    /// 句柄是否已有去处：重定向到文件/管道，或确实是已附着的控制台。
    /// （GUI 子系统的进程可能拿到父进程的控制台句柄却并未附着，此时 GetConsoleMode 失败。）
    fn has_output(h: Handle) -> bool {
        if !valid(h) {
            return false;
        }
        match unsafe { GetFileType(h) } {
            FILE_TYPE_DISK | FILE_TYPE_PIPE => true,
            FILE_TYPE_CHAR => {
                let mut mode = 0u32;
                (unsafe { GetConsoleMode(h, &mut mode) }) != 0
            }
            _ => false,
        }
    }

    pub fn attach() {
        if has_output(unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }) {
            return;
        }
        let attached =
            unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0 || unsafe { AllocConsole() } != 0;
        if !attached {
            return;
        }
        for (std_handle, name) in [
            (STD_OUTPUT_HANDLE, "CONOUT$"),
            (STD_ERROR_HANDLE, "CONOUT$"),
            (STD_INPUT_HANDLE, "CONIN$"),
        ] {
            let h = open_console(name);
            if valid(h) {
                unsafe { SetStdHandle(std_handle, h) };
            }
        }
    }
}
