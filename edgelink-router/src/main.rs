#![cfg(windows)]
#![windows_subsystem = "windows"]

mod app;
mod elevation;
mod memory_log;
#[allow(dead_code)]
mod protocol;
mod service_control;
mod settings;
mod tray;
mod ui;
#[allow(dead_code)]
mod windows_net;

use app::RouteClientApp;
use gpui_kit::{
    assets::Assets,
    component::theme::{Theme, ThemeMode},
    *,
};
use settings::Settings;
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc, sync::mpsc, time::Duration};

fn main() {
    if handle_elevated_command() {
        return;
    }

    let (tray_sender, tray_receiver) = mpsc::channel();
    let mut tray_icon = match tray::TrayIcon::start(tray_sender) {
        Ok(icon) => Some(icon),
        Err(error) => {
            eprintln!("系统托盘初始化失败：{error}");
            None
        }
    };
    let tray_enabled = tray_icon.is_some();
    let app = application();
    let app = if tray_enabled {
        app.with_quit_mode(QuitMode::Explicit)
    } else {
        app
    };
    app.with_assets(Assets).run(move |cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let window_handle = open_config_window(cx, |window, cx| {
            cx.new(|cx| RouteClientApp::new(window, cx))
        });
        let window_state = Rc::new(RefCell::new(window_handle));
        let closed_state = Rc::downgrade(&window_state);
        cx.on_window_closed(move |_, window_id| {
            if let Some(state) = closed_state.upgrade() {
                let is_tracked_window = state
                    .borrow()
                    .is_some_and(|handle| handle.window_id() == window_id);
                if is_tracked_window {
                    *state.borrow_mut() = None;
                }
            }
        })
        .detach();

        if tray_enabled {
            let window_state = window_state.clone();
            cx.spawn(async move |cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                while let Ok(action) = tray_receiver.try_recv() {
                    let should_quit = cx.update(|cx| handle_tray_action(action, cx, &window_state));
                    if should_quit {
                        return;
                    }
                }
            })
            .detach();
        }
    });
    if let Some(icon) = tray_icon.as_mut() {
        icon.stop();
    }
}

fn open_config_window(
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<RouteClientApp>,
) -> Option<AnyWindowHandle> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1090.), px(880.)), cx)),
        is_resizable: false,
        ..WindowOptions::default()
    };
    open_window(options, cx, build)
        .map(|(handle, _)| {
            let _ = handle.update(cx, |_, window, _| {
                window.resize(size(px(1090.), px(880.)));
            });
            tray::apply_window_icon();
            handle
        })
        .ok()
}

fn show_config_window(cx: &mut App, window_state: &Rc<RefCell<Option<AnyWindowHandle>>>) {
    if let Some(handle) = *window_state.borrow() {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
        *window_state.borrow_mut() = None;
    }

    if let Some(handle) = open_config_window(cx, |window, cx| {
        cx.new(|cx| RouteClientApp::new(window, cx))
    }) {
        *window_state.borrow_mut() = Some(handle);
    }
}

fn handle_tray_action(
    action: tray::TrayAction,
    cx: &mut App,
    window_state: &Rc<RefCell<Option<AnyWindowHandle>>>,
) -> bool {
    match action {
        tray::TrayAction::OpenConfig => show_config_window(cx, window_state),
        tray::TrayAction::StartService => {
            if let Err(error) = elevation::launch("--elevated start") {
                elevation::show_result("Edge Board Route", Err(error));
            }
        }
        tray::TrayAction::StopService => {
            if let Err(error) = elevation::launch("--elevated stop") {
                elevation::show_result("Edge Board Route", Err(error));
            }
        }
        tray::TrayAction::Exit => {
            cx.quit();
            return true;
        }
    }
    false
}

fn handle_elevated_command() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--elevated")) {
        return false;
    }
    let Some(operation) = args.next().and_then(|value| value.into_string().ok()) else {
        elevation::show_result("EdgeLink", Err("缺少操作名称".into()));
        return true;
    };
    let needs_settings = matches!(operation.as_str(), "save-config" | "install");
    let config_path = if needs_settings {
        args.next().map(PathBuf::from)
    } else {
        None
    };
    let result = if needs_settings && config_path.is_none() {
        Err("缺少临时配置文件路径".into())
    } else {
        config_path
            .as_deref()
            .map(save_temporary_settings)
            .unwrap_or(Ok(()))
            .and_then(|()| match operation.as_str() {
                "save-config" => Ok(()),
                "install" => service_control::install_and_start(),
                "start" => service_control::start(),
                "stop" => service_control::stop(),
                "uninstall" => service_control::uninstall(),
                _ => Err(format!("不支持的操作：{operation}")),
            })
    };
    if let Some(path) = config_path {
        let _ = fs::remove_file(path);
    }
    elevation::show_result("EdgeLink", result);
    true
}

fn save_temporary_settings(path: &std::path::Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("读取暂存配置失败：{error}"))?;
    let settings: Settings =
        serde_json::from_slice(&bytes).map_err(|error| format!("解析暂存配置失败：{error}"))?;
    settings.save()
}
