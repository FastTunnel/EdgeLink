use std::{
    ffi::OsStr,
    iter::once,
    os::windows::ffi::OsStrExt,
    sync::mpsc::Sender,
    thread::{self, JoinHandle},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, FindWindowExW, GetCursorPos, GetMessageW,
            GetSystemMetrics, GetWindowThreadProcessId, LoadIconW, LoadImageW, PostMessageW,
            PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, TrackPopupMenu,
            TranslateMessage, ICON_BIG, ICON_SMALL, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE,
            LR_SHARED, MF_SEPARATOR, MF_STRING, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON,
            TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK,
            WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_SETICON, WNDCLASSW,
        },
    },
};

const TRAY_CALLBACK: u32 = WM_APP + 21;
const ICON_ID: u32 = 1;
const MENU_OPEN_CONFIG: i32 = 1001;
const MENU_START_SERVICE: i32 = 1002;
const MENU_STOP_SERVICE: i32 = 1003;
const MENU_EXIT: i32 = 1004;

#[derive(Clone, Copy)]
pub enum TrayAction {
    OpenConfig,
    StartService,
    StopService,
    Exit,
}

pub struct TrayIcon {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl TrayIcon {
    pub fn start(sender: Sender<TrayAction>) -> Result<Self, String> {
        ACTION_SENDER
            .set(sender.clone())
            .map_err(|_| "系统托盘已经初始化".to_owned())?;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("edge-route-tray".into())
            .spawn(move || tray_thread(ready_tx))
            .map_err(|error| format!("无法创建托盘线程：{error}"))?;
        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => {
                let _ = thread.join();
                Err(format!("托盘线程初始化失败：{error}"))
            }
        }
    }

    pub fn stop(&mut self) {
        if self.hwnd != 0 {
            unsafe {
                PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
            }
            self.hwnd = 0;
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.stop();
    }
}

fn tray_thread(ready: std::sync::mpsc::SyncSender<Result<isize, String>>) {
    let result = unsafe { initialize_tray(&ready) };
    if let Err(error) = result {
        let _ = ready.send(Err(error));
    }
}

unsafe fn initialize_tray(
    ready: &std::sync::mpsc::SyncSender<Result<isize, String>>,
) -> Result<(), String> {
    let class_name = wide("EdgeLinkRouteTrayWindow");
    let instance = GetModuleHandleW(std::ptr::null());
    if instance.is_null() {
        return Err("GetModuleHandleW 获取程序句柄失败".into());
    }

    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 {
        return Err("RegisterClassW 注册托盘窗口失败".into());
    }

    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        class_name.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        windows_sys::Win32::UI::WindowsAndMessaging::HWND_MESSAGE,
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        return Err("CreateWindowExW 创建托盘窗口失败".into());
    }

    let mut icon = NOTIFYICONDATAW::default();
    icon.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    icon.hWnd = hwnd;
    icon.uID = ICON_ID;
    icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    icon.uCallbackMessage = TRAY_CALLBACK;
    icon.hIcon = load_app_icon(
        instance,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
    );
    copy_wide("EdgeLink 路由助手", &mut icon.szTip);

    if Shell_NotifyIconW(NIM_ADD, &icon) == 0 {
        DestroyWindow(hwnd);
        return Err("Shell_NotifyIconW 添加托盘图标失败".into());
    }

    if ready.send(Ok(hwnd as isize)).is_err() {
        Shell_NotifyIconW(NIM_DELETE, &icon);
        DestroyWindow(hwnd);
        return Ok(());
    }

    let mut message = windows_sys::Win32::UI::WindowsAndMessaging::MSG::default();
    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }

    Shell_NotifyIconW(NIM_DELETE, &icon);
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == TRAY_CALLBACK {
        let event = lparam as u32;
        if event == WM_RBUTTONUP {
            show_menu(hwnd);
            return 0;
        }
        if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
            send_action(TrayAction::OpenConfig);
            return 0;
        }
    }

    if message == WM_CLOSE {
        DestroyWindow(hwnd);
        return 0;
    }
    if message == WM_DESTROY {
        PostQuitMessage(0);
        return 0;
    }

    DefWindowProcW(hwnd, message, wparam, lparam)
}

unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    append_item(menu, MENU_OPEN_CONFIG, "打开配置");
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    match crate::service_control::status() {
        Ok(Some(windows_service::service::ServiceState::Running)) => {
            append_item(menu, MENU_STOP_SERVICE, "停止后台服务");
        }
        Ok(Some(_)) => append_item(menu, MENU_START_SERVICE, "启动后台服务"),
        Ok(None) => append_item(menu, MENU_OPEN_CONFIG, "服务未安装：打开配置"),
        Err(_) => append_item(menu, MENU_OPEN_CONFIG, "查看服务状态"),
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    append_item(menu, MENU_EXIT, "退出托盘程序");

    let mut point = POINT::default();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match selected {
        MENU_OPEN_CONFIG => send_action(TrayAction::OpenConfig),
        MENU_START_SERVICE => send_action(TrayAction::StartService),
        MENU_STOP_SERVICE => send_action(TrayAction::StopService),
        MENU_EXIT => send_action(TrayAction::Exit),
        _ => {}
    }
}

unsafe fn append_item(
    menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
    id: i32,
    label: &str,
) {
    let label = wide(label);
    AppendMenuW(menu, MF_STRING, id as usize, label.as_ptr());
}

fn send_action(action: TrayAction) {
    let _ = ACTION_SENDER.get().map(|sender| sender.send(action));
}

static ACTION_SENDER: std::sync::OnceLock<Sender<TrayAction>> = std::sync::OnceLock::new();

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

fn copy_wide<const N: usize>(value: &str, destination: &mut [u16; N]) {
    let encoded = value.encode_utf16().take(N.saturating_sub(1));
    for (slot, character) in destination.iter_mut().zip(encoded) {
        *slot = character;
    }
}

unsafe fn load_app_icon(
    instance: windows_sys::Win32::Foundation::HINSTANCE,
    width: i32,
    height: i32,
) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    let flags = if width == 0 && height == 0 {
        LR_DEFAULTSIZE | LR_SHARED
    } else {
        LR_SHARED
    };
    let handle = LoadImageW(
        instance,
        1usize as *const u16,
        IMAGE_ICON,
        width,
        height,
        flags,
    );
    if !handle.is_null() {
        return handle as windows_sys::Win32::UI::WindowsAndMessaging::HICON;
    }
    let fallback = LoadIconW(instance, 1usize as *const u16);
    if !fallback.is_null() {
        return fallback;
    }
    LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
}

pub fn apply_window_icon() {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        if instance.is_null() {
            return;
        }
        let small_icon = load_app_icon(
            instance,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
        );
        let big_icon = load_app_icon(
            instance,
            GetSystemMetrics(SM_CXICON),
            GetSystemMetrics(SM_CYICON),
        );
        let class_name = wide("GpuiFast::Window");
        let current_pid = std::process::id();
        let mut hwnd: HWND = std::ptr::null_mut();
        loop {
            hwnd = FindWindowExW(
                std::ptr::null_mut(),
                hwnd,
                class_name.as_ptr(),
                std::ptr::null(),
            );
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == current_pid {
                if !small_icon.is_null() {
                    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small_icon as isize);
                }
                if !big_icon.is_null() {
                    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big_icon as isize);
                }
            }
        }
    }
}
