use std::{
    ffi::OsString,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use windows_service::{
    service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
    Error,
};

pub const SERVICE_NAME: &str = "EdgeLinkRouteService";
const DISPLAY_NAME: &str = "EdgeLink Route Service";

pub fn status() -> Result<Option<ServiceState>, String> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(service) => service
            .query_status()
            .map(|status| Some(status.current_state))
            .map_err(|error| format!("无法读取服务状态：{error}")),
        Err(Error::Winapi(error))
            if error.raw_os_error()
                == Some(windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST as i32) =>
        {
            Ok(None)
        }
        Err(error) => Err(format!("无法查询服务：{error}")),
    }
}

pub fn install_and_start() -> Result<(), String> {
    let binary = service_binary_path()?;
    install_and_start_with_binary(binary)
}

pub fn install_and_start_with_binary(binary: PathBuf) -> Result<(), String> {
    let manager = connect(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    if !binary.is_file() {
        return Err(format!("服务程序不存在：{}", binary.display()));
    }
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: binary,
        launch_arguments: Vec::new(),
        dependencies: Vec::new(),
        account_name: None,
        account_password: None,
    };
    let access =
        ServiceAccess::START | ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::CHANGE_CONFIG;
    let service = match manager.create_service(&info, access) {
        Ok(service) => service,
        Err(Error::Winapi(err)) if err.raw_os_error() == Some(1073) => {
            // ERROR_SERVICE_EXISTS (1073): open existing service, stop if running, and update config
            let existing = manager
                .open_service(SERVICE_NAME, access)
                .map_err(|e| format!("打开已存在的服务失败：{e}"))?;
            if let Ok(st) = existing.query_status() {
                if st.current_state != ServiceState::Stopped {
                    let _ = existing.stop();
                    let _ = wait_until_stopped(&existing);
                }
            }
            existing
                .change_config(&info)
                .map_err(|e| format!("更新现有服务配置失败：{e}"))?;
            existing
        }
        Err(error) => return Err(format!("服务安装失败：{error}")),
    };
    service
        .set_description("Discovers the edge Linux board and maintains its Windows routes.")
        .map_err(|error| format!("服务已注册，但设置说明失败：{error}"))?;
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(15),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(30),
                },
            ]),
        })
        .map_err(|error| format!("服务已注册，但设置故障重启策略失败：{error}"))?;
    service
        .start::<OsString>(&[])
        .map_err(|error| format!("服务已安装为开机启动，但启动失败：{error}"))
}

pub fn start() -> Result<(), String> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .map_err(|error| format!("打开服务失败：{error}"))?;
    service
        .start::<OsString>(&[])
        .map_err(|error| format!("启动服务失败：{error}"))
}

pub fn stop() -> Result<(), String> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
        )
        .map_err(|error| format!("打开服务失败：{error}"))?;
    if service
        .query_status()
        .map_err(|error| format!("读取服务状态失败：{error}"))?
        .current_state
        == ServiceState::Stopped
    {
        return Ok(());
    }
    service
        .stop()
        .map_err(|error| format!("停止服务失败：{error}"))?;
    wait_until_stopped(&service)
}

pub fn uninstall() -> Result<(), String> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(|error| format!("打开服务失败：{error}"))?;
    if service
        .query_status()
        .map_err(|error| format!("读取服务状态失败：{error}"))?
        .current_state
        != ServiceState::Stopped
    {
        service
            .stop()
            .map_err(|error| format!("停止服务失败，未卸载：{error}"))?;
        wait_until_stopped(&service)?;
    }
    service
        .delete()
        .map_err(|error| format!("卸载服务失败：{error}"))
}

fn wait_until_stopped(service: &windows_service::service::Service) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = service
            .query_status()
            .map_err(|error| format!("等待服务停止时读取状态失败：{error}"))?;
        if status.current_state == ServiceState::Stopped {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("等待服务停止超时".into());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn connect(access: ServiceManagerAccess) -> Result<ServiceManager, String> {
    ServiceManager::local_computer(None::<&str>, access).map_err(|error| {
        format!(
            "连接 Windows 服务管理器失败：{}",
            explain_windows_error(&error)
        )
    })
}

fn explain_windows_error(error: &Error) -> String {
    match error {
        Error::Winapi(io_error) => match io_error.raw_os_error() {
            Some(5) => "Windows 错误 5（拒绝访问）。请在系统 UAC 提示中允许管理员权限。".into(),
            Some(code) => format!("Windows 错误 {code}：{io_error}"),
            None => io_error.to_string(),
        },
        _ => error.to_string(),
    }
}

fn service_binary_path() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map(|path| path.with_file_name("edgelink-service.exe"))
        .map_err(|error| format!("无法定位配置程序所在目录：{error}"))
}
