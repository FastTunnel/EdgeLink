#![cfg(windows)]
#![windows_subsystem = "windows"]

#[path = "../elevation.rs"]
#[allow(dead_code)]
mod elevation;
#[path = "../protocol.rs"]
#[allow(dead_code)]
mod protocol;
#[path = "../service_control.rs"]
#[allow(dead_code)]
mod service_control;
#[path = "../settings.rs"]
#[allow(dead_code)]
mod settings;
#[path = "../windows_net.rs"]
#[allow(dead_code)]
mod windows_net;

use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    iter::once,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    ptr,
};
use windows_sys::Win32::UI::{
    Shell::ShellExecuteW,
    WindowsAndMessaging::{
        MessageBoxW, IDNO, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK,
        MB_YESNO, MB_YESNOCANCEL, SW_SHOWNORMAL,
    },
};

const MAGIC: &[u8; 16] = b"EDGEROUTESETUP01";
const APP_TITLE: &str = "EdgeLink 安装与卸载程序";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct EmbeddedPayload {
    client_exe: Vec<u8>,
    service_exe: Vec<u8>,
    icon_ico: Vec<u8>,
    stub_exe: Vec<u8>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("--pack") {
        if let Err(err) = run_pack(&args[1..]) {
            eprintln!("Pack installer failed: {err}");
            std::process::exit(1);
        }
        return;
    }

    let exe_name = env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();

    let is_silent = args.iter().any(|a| a == "--silent" || a == "/S");
    let mut is_uninstall = exe_name.contains("uninstall")
        || args
            .iter()
            .any(|a| a == "--uninstall" || a == "--elevated-uninstall");
    let is_elevated = args
        .iter()
        .any(|a| a == "--elevated-install" || a == "--elevated-uninstall");

    // If user double-clicked the setup .exe while the app is already installed,
    // offer [Yes = Reinstall/Upgrade], [No = Uninstall], [Cancel = Exit]
    if !is_uninstall && !is_elevated && !is_silent && is_already_installed() {
        let install_dir = target_install_dir();
        let prompt = format!(
            "检测到系统已安装「EdgeLink 路由助手 (EdgeLink Router) v{APP_VERSION}」\n\
             已安装位置：{}\n\n\
             请选择要执行的操作：\n\
             • 点击【是 (Y)】：重新安装 / 覆盖更新程序与服务\n\
             • 点击【否 (N)】：彻底卸载程序、快捷方式与后台路由服务\n\
             • 点击【取消】：退出向导",
            install_dir.display()
        );
        match confirm_yes_no_cancel(&prompt) {
            Some(true) => {
                // Proceed with reinstall
            }
            Some(false) => {
                is_uninstall = true;
            }
            None => return,
        }
    }

    if is_uninstall {
        if !is_elevated {
            if !is_silent
                && !confirm_dialog(
                    "确定要彻底卸载「EdgeLink 路由助手 (EdgeLink Router)」吗？\n\n\
                     卸载将执行以下清理：\n\
                     • 停止并移除后台服务 EdgeLinkRouteService\n\
                     • 清理本服务接管的开发板路由并恢复网卡 DHCP\n\
                     • 删除桌面与开始菜单快捷方式及程序目录",
                )
            {
                return;
            }
            let flag = if is_silent {
                "--elevated-uninstall --silent"
            } else {
                "--elevated-uninstall"
            };
            if let Err(err) = elevation::launch(flag) {
                show_error(&format!("无法获取管理员权限执行卸载：{err}"));
            }
            return;
        }
        match perform_uninstall() {
            Ok(()) => {
                if !is_silent {
                    show_info("「EdgeLink 路由助手」及后台路由守护服务已成功卸载。");
                }
            }
            Err(err) => show_error(&format!("卸载过程中出现错误：{err}")),
        }
        return;
    }

    // Install flow
    if !is_elevated {
        let install_dir = target_install_dir();
        if !is_silent && !is_already_installed() {
            let prompt = format!(
                "即将安装「EdgeLink 路由助手 (EdgeLink Router) v{APP_VERSION}」\n\n\
                 安装目录：{}\n\n\
                 安装内容：\n\
                 • 客户端图形配置界面 (edgelink-router.exe)\n\
                 • Windows 后台路由守护服务 (edgelink-service.exe)\n\
                 • 独立卸载程序 (uninstall.exe)\n\
                 • 自动注册并启动开机自启服务 EdgeLinkRouteService\n\
                 • 创建桌面与开始菜单快捷方式及系统卸载项\n\n\
                 是否继续安装？",
                install_dir.display()
            );
            if !confirm_dialog(&prompt) {
                return;
            }
        }
        let flag = if is_silent {
            "--elevated-install --silent"
        } else {
            "--elevated-install"
        };
        if let Err(err) = elevation::launch(flag) {
            show_error(&format!("无法请求管理员权限进行安装：{err}"));
        }
        return;
    }

    match perform_install() {
        Ok(client_path) => {
            if is_silent {
                return;
            }
            let launch_now = confirm_dialog(
                "「EdgeLink 路由助手」安装完成，后台路由服务已启动！\n\
                 （后续可通过开始菜单、设置->已安装的应用、或再次运行本安装包进行卸载）\n\n\
                 是否立即打开客户端配置界面？",
            );
            if launch_now {
                launch_client_non_elevated(&client_path);
            }
        }
        Err(err) => {
            show_error(&format!("安装失败：{err}"));
            std::process::exit(1);
        }
    }
}

fn is_already_installed() -> bool {
    let dir = target_install_dir();
    dir.join("edgelink-router.exe").is_file()
        || dir.join("windows-route-client.exe").is_file()
        || dir.join("uninstall.exe").is_file()
}

fn run_pack(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err(
            "用法: edgelink-setup --pack <client.exe> <service.exe> <app.ico> <out_setup.exe>"
                .into(),
        );
    }
    let current_exe = env::current_exe().map_err(|e| format!("无法读取当前程序路径: {e}"))?;
    let stub_bytes = read_clean_stub(&current_exe)?;
    let client_bytes = fs::read(&args[0]).map_err(|e| format!("读取 client.exe 失败: {e}"))?;
    let service_bytes = fs::read(&args[1]).map_err(|e| format!("读取 service.exe 失败: {e}"))?;
    let icon_bytes = fs::read(&args[2]).map_err(|e| format!("读取 app.ico 失败: {e}"))?;
    let out_path = PathBuf::from(&args[3]);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败: {e}"))?;
    }

    let mut out = File::create(&out_path).map_err(|e| format!("创建安装包文件失败: {e}"))?;
    out.write_all(&stub_bytes)
        .map_err(|e| format!("写入安装引导头失败: {e}"))?;

    let trailer_offset = stub_bytes.len() as u64;
    write_blob(&mut out, &client_bytes)?;
    write_blob(&mut out, &service_bytes)?;
    write_blob(&mut out, &icon_bytes)?;
    out.write_all(&trailer_offset.to_le_bytes())
        .map_err(|e| format!("写入偏移量失败: {e}"))?;
    out.write_all(MAGIC)
        .map_err(|e| format!("写入安装包签名失败: {e}"))?;
    out.flush().map_err(|e| format!("刷新文件失败: {e}"))?;
    Ok(())
}

fn write_blob(out: &mut File, bytes: &[u8]) -> Result<(), String> {
    let len = bytes.len() as u64;
    out.write_all(&len.to_le_bytes())
        .map_err(|e| format!("写入数据长度失败: {e}"))?;
    out.write_all(bytes)
        .map_err(|e| format!("写入数据载荷失败: {e}"))
}

fn read_blob(file: &mut File) -> Result<Vec<u8>, String> {
    let mut len_buf = [0u8; 8];
    file.read_exact(&mut len_buf)
        .map_err(|e| format!("读取载荷长度失败: {e}"))?;
    let len = u64::from_le_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    file.read_exact(&mut data)
        .map_err(|e| format!("读取载荷内容失败: {e}"))?;
    Ok(data)
}

fn read_clean_stub(exe_path: &Path) -> Result<Vec<u8>, String> {
    let mut file = File::open(exe_path).map_err(|e| format!("打开程序文件失败: {e}"))?;
    let total_len = file
        .metadata()
        .map_err(|e| format!("读取文件元数据失败: {e}"))?
        .len();
    if total_len > 24 {
        file.seek(SeekFrom::End(-24))
            .map_err(|e| format!("定位文件尾部失败: {e}"))?;
        let mut footer = [0u8; 24];
        if file.read_exact(&mut footer).is_ok() && &footer[8..24] == MAGIC {
            let offset = u64::from_le_bytes(footer[0..8].try_into().unwrap());
            if offset < total_len {
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| format!("定位文件头部失败: {e}"))?;
                let mut stub = vec![0u8; offset as usize];
                file.read_exact(&mut stub)
                    .map_err(|e| format!("读取引导头失败: {e}"))?;
                return Ok(stub);
            }
        }
    }
    fs::read(exe_path).map_err(|e| format!("读取程序文件失败: {e}"))
}

fn load_payload() -> Result<EmbeddedPayload, String> {
    let current_exe = env::current_exe().map_err(|e| format!("无法定位当前安装程序路径：{e}"))?;
    let mut file = File::open(&current_exe).map_err(|e| format!("无法打开安装程序自身：{e}"))?;
    let total_len = file
        .metadata()
        .map_err(|e| format!("无法读取安装程序大小：{e}"))?
        .len();

    if total_len > 24 {
        file.seek(SeekFrom::End(-24))
            .map_err(|e| format!("读取安装包尾部失败：{e}"))?;
        let mut footer = [0u8; 24];
        if file.read_exact(&mut footer).is_ok() && &footer[8..24] == MAGIC {
            let offset = u64::from_le_bytes(footer[0..8].try_into().unwrap());
            if offset < total_len - 24 {
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| format!("读取引导头失败：{e}"))?;
                let mut stub_exe = vec![0u8; offset as usize];
                file.read_exact(&mut stub_exe)
                    .map_err(|e| format!("读取引导程序失败：{e}"))?;

                let client_exe = read_blob(&mut file)?;
                let service_exe = read_blob(&mut file)?;
                let icon_ico = read_blob(&mut file)?;
                return Ok(EmbeddedPayload {
                    client_exe,
                    service_exe,
                    icon_ico,
                    stub_exe,
                });
            }
        }
    }

    let dir = current_exe
        .parent()
        .ok_or_else(|| "无法确定当前程序所在目录".to_owned())?;
    let client_exe = fs::read(dir.join("edgelink-router.exe"))
        .or_else(|_| fs::read(dir.join("windows-route-client.exe")))
        .map_err(|e| format!("未找到内嵌载荷且同目录缺少 edgelink-router.exe：{e}"))?;
    let service_exe = fs::read(dir.join("edgelink-service.exe"))
        .or_else(|_| fs::read(dir.join("windows-route-service.exe")))
        .map_err(|e| format!("未找到内嵌载荷且同目录缺少 edgelink-service.exe：{e}"))?;
    let icon_ico = fs::read(dir.join("app.ico"))
        .or_else(|_| fs::read(dir.join("../../assets/app.ico")))
        .unwrap_or_default();
    let stub_exe = fs::read(&current_exe).unwrap_or_default();
    Ok(EmbeddedPayload {
        client_exe,
        service_exe,
        icon_ico,
        stub_exe,
    })
}

fn perform_install() -> Result<PathBuf, String> {
    let payload = load_payload()?;
    let install_dir = target_install_dir();

    // 1. Stop existing service and running GUI client before overwriting binaries
    let _ = service_control::stop();
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", "edgelink-router.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", "windows-route-client.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    std::thread::sleep(std::time::Duration::from_millis(400));

    // 2. Write files to C:\Program Files\EdgeLink
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("无法创建安装目录 {}：{e}", install_dir.display()))?;

    let client_path = install_dir.join("edgelink-router.exe");
    let service_path = install_dir.join("edgelink-service.exe");
    let uninstall_path = install_dir.join("uninstall.exe");
    let icon_path = install_dir.join("app.ico");

    fs::write(&client_path, &payload.client_exe)
        .map_err(|e| format!("写入 {} 失败：{e}", client_path.display()))?;
    fs::write(&service_path, &payload.service_exe)
        .map_err(|e| format!("写入 {} 失败：{e}", service_path.display()))?;
    if !payload.stub_exe.is_empty() {
        fs::write(&uninstall_path, &payload.stub_exe)
            .map_err(|e| format!("写入 {} 失败：{e}", uninstall_path.display()))?;
    }
    if !payload.icon_ico.is_empty() {
        let _ = fs::write(&icon_path, &payload.icon_ico);
    }

    // 3. Ensure default settings.json exists in %PROGRAMDATA%\EdgeLink\settings.json
    if settings::settings_path().is_some_and(|p| !p.exists()) {
        let default_settings = settings::Settings::default();
        let _ = default_settings.save();
    }

    // 4. Register (or update) and start EdgeLinkRouteService
    service_control::install_and_start_with_binary(service_path)?;

    // 5. Create Desktop and Start Menu shortcuts (including Uninstall shortcut) & Uninstall registry entry
    create_shortcuts(&client_path, &uninstall_path, &icon_path);
    let total_kb =
        ((payload.client_exe.len() + payload.service_exe.len() + payload.stub_exe.len()) / 1024)
            as u32;
    register_uninstaller(&install_dir, &uninstall_path, &client_path, total_kb);

    Ok(client_path)
}

fn perform_uninstall() -> Result<(), String> {
    // 1. Clean up managed routes on configured interface before removing service
    let cfg = settings::Settings::load();
    if let Ok((if_index, _)) = windows_net::interface_addresses(&cfg.interface) {
        let _ = windows_net::remove_routes(if_index);
    }

    // 2. Stop and delete EdgeLinkRouteService (and clean legacy EdgeBoardRouteService if present)
    let _ = service_control::uninstall();

    // 3. Terminate running GUI client
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", "edgelink-router.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", "windows-route-client.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    std::thread::sleep(std::time::Duration::from_millis(400));

    // 4. Remove Desktop & Start Menu shortcuts and Windows Uninstall registry key
    remove_shortcuts();
    unregister_uninstaller();

    // 5. Remove %PROGRAMDATA%\EdgeLink configuration directory
    if let Some(cfg_path) = settings::settings_path() {
        if let Some(cfg_dir) = cfg_path.parent() {
            let _ = fs::remove_dir_all(cfg_dir);
        }
    }

    // 6. Remove installed files in C:\Program Files\EdgeLink
    let install_dir = target_install_dir();
    let _ = fs::remove_file(install_dir.join("edgelink-router.exe"));
    let _ = fs::remove_file(install_dir.join("edgelink-service.exe"));
    let _ = fs::remove_file(install_dir.join("windows-route-client.exe"));
    let _ = fs::remove_file(install_dir.join("windows-route-service.exe"));
    let _ = fs::remove_file(install_dir.join("app.ico"));

    let uninstall_exe = install_dir.join("uninstall.exe");
    let cleanup_cmd = format!(
        "ping 127.0.0.1 -n 2 > nul & del /f /q \"{}\" > nul 2> nul & rmdir /s /q \"{}\" > nul 2> nul",
        uninstall_exe.display(),
        install_dir.display()
    );
    let _ = Command::new("cmd.exe")
        .args(["/C", &cleanup_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    Ok(())
}

fn target_install_dir() -> PathBuf {
    env::var_os("ProgramW6432")
        .or_else(|| env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
        .join("EdgeLink")
}

fn desktop_shortcut_path() -> Option<PathBuf> {
    env::var_os("PUBLIC")
        .map(|p| PathBuf::from(p).join("Desktop").join("EdgeLink 路由助手.lnk"))
}

fn start_menu_dir() -> Option<PathBuf> {
    env::var_os("PROGRAMDATA").map(|p| {
        PathBuf::from(p)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("EdgeLink")
    })
}

fn create_shortcuts(target_exe: &Path, uninstall_exe: &Path, icon_path: &Path) {
    let icon = if icon_path.exists() {
        icon_path
    } else {
        target_exe
    };
    let working_dir = target_exe.parent().unwrap_or_else(|| Path::new("C:\\"));

    if let Some(desktop_lnk) = desktop_shortcut_path() {
        create_single_shortcut(
            &desktop_lnk,
            target_exe,
            "",
            working_dir,
            icon,
            "EdgeLink 路由助手 - 自动发现开发板并维护路由表",
        );
    }

    if let Some(menu_dir) = start_menu_dir() {
        let _ = fs::create_dir_all(&menu_dir);
        create_single_shortcut(
            &menu_dir.join("EdgeLink 路由助手.lnk"),
            target_exe,
            "",
            working_dir,
            icon,
            "EdgeLink 路由助手 - 自动发现开发板并维护路由表",
        );
        create_single_shortcut(
            &menu_dir.join("卸载 EdgeLink 路由助手.lnk"),
            uninstall_exe,
            "--uninstall",
            working_dir,
            icon,
            "卸载 EdgeLink 路由助手及后台路由服务",
        );
    }
}

fn create_single_shortcut(
    link_path: &Path,
    target_path: &Path,
    arguments: &str,
    working_dir: &Path,
    icon_path: &Path,
    description: &str,
) {
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.Arguments = '{}'; \
         $s.WorkingDirectory = '{}'; \
         $s.IconLocation = '{},0'; \
         $s.Description = '{}'; \
         $s.Save()",
        link_path.display().to_string().replace('\'', "''"),
        target_path.display().to_string().replace('\'', "''"),
        arguments.replace('\'', "''"),
        working_dir.display().to_string().replace('\'', "''"),
        icon_path.display().to_string().replace('\'', "''"),
        description.replace('\'', "''"),
    );
    let _ = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn remove_shortcuts() {
    if let Some(desktop_lnk) = desktop_shortcut_path() {
        let _ = fs::remove_file(desktop_lnk);
    }
    // Also remove legacy desktop link if present
    if let Some(pub_dir) = env::var_os("PUBLIC") {
        let legacy = PathBuf::from(pub_dir).join("Desktop").join("开发板网络助手.lnk");
        let _ = fs::remove_file(legacy);
    }

    if let Some(menu_dir) = start_menu_dir() {
        let _ = fs::remove_dir_all(menu_dir);
    }
    // Also clean legacy start menu directories
    if let Some(prog_data) = env::var_os("PROGRAMDATA") {
        let legacy_dir = PathBuf::from(&prog_data)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("开发板网络助手");
        let _ = fs::remove_dir_all(legacy_dir);
        let legacy_lnk = PathBuf::from(&prog_data)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("开发板网络助手.lnk");
        let _ = fs::remove_file(legacy_lnk);
    }
}

fn register_uninstaller(
    install_dir: &Path,
    uninstall_exe: &Path,
    client_exe: &Path,
    size_kb: u32,
) {
    let key = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\EdgeLink";
    let uninstall_str = format!("\"{}\" --uninstall", uninstall_exe.display());
    let quiet_uninstall_str = format!("\"{}\" --uninstall --silent", uninstall_exe.display());
    let icon_str = format!("{},0", client_exe.display());
    let size_str = size_kb.to_string();
    let entries = [
        ("DisplayName", "REG_SZ", "EdgeLink 路由助手 (EdgeLink Router)"),
        ("DisplayVersion", "REG_SZ", APP_VERSION),
        ("Publisher", "REG_SZ", "EdgeLink Project"),
        ("InstallLocation", "REG_SZ", &install_dir.display().to_string()),
        ("DisplayIcon", "REG_SZ", &icon_str),
        ("UninstallString", "REG_SZ", &uninstall_str),
        ("QuietUninstallString", "REG_SZ", &quiet_uninstall_str),
        ("EstimatedSize", "REG_DWORD", &size_str),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ];
    for (name, reg_type, value) in entries {
        let _ = Command::new("reg.exe")
            .args(["add", key, "/v", name, "/t", reg_type, "/d", value, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}

fn unregister_uninstaller() {
    let key = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\EdgeLink";
    let _ = Command::new("reg.exe")
        .args(["delete", key, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    // Also delete legacy registry key if exists
    let legacy_key = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\EdgeBoardRouteClient";
    let _ = Command::new("reg.exe")
        .args(["delete", legacy_key, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn launch_client_non_elevated(client_exe: &Path) {
    let _ = unsafe {
        let verb = wide("open");
        let file = wide("explorer.exe");
        let params = wide(&format!("\"{}\"", client_exe.display()));
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

fn confirm_dialog(message: &str) -> bool {
    let title = wide(APP_TITLE);
    let body = wide(message);
    let ans = unsafe {
        MessageBoxW(
            ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    ans == IDYES
}

fn confirm_yes_no_cancel(message: &str) -> Option<bool> {
    let title = wide(APP_TITLE);
    let body = wide(message);
    let ans = unsafe {
        MessageBoxW(
            ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_YESNOCANCEL | MB_ICONQUESTION,
        )
    };
    if ans == IDYES {
        Some(true)
    } else if ans == IDNO {
        Some(false)
    } else {
        None
    }
}

fn show_info(message: &str) -> Vec<u16> {
    let title = wide(APP_TITLE);
    let body = wide(message);
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
    title
}

fn show_error(message: &str) {
    let title = wide(APP_TITLE);
    let body = wide(message);
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}
