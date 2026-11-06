use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt, ptr};
use windows_sys::Win32::UI::{
    Shell::ShellExecuteW,
    WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL},
};

pub fn launch(arguments: &str) -> Result<(), String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("无法定位配置程序：{error}"))?;
    let verb = wide("runas");
    let executable = wide_os(executable.as_os_str());
    let arguments = wide(arguments);
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            executable.as_ptr(),
            arguments.as_ptr(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        let code = result as isize;
        let detail = if code == 5 || code == 1223 {
            "UAC 已取消或拒绝管理员权限".to_owned()
        } else {
            format!("Windows 启动错误 {code}")
        };
        Err(detail)
    } else {
        Ok(())
    }
}

pub fn show_result(title: &str, result: Result<(), String>) {
    let (message, icon) = match result {
        Ok(()) => ("操作已完成".to_owned(), MB_ICONINFORMATION),
        Err(error) => (error, MB_ICONERROR),
    };
    let title = wide(title);
    let message = wide(&message);
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | icon,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

fn wide_os(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(once(0)).collect()
}
