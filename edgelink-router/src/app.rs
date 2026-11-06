use crate::{
    elevation,
    memory_log::{LogLevel, MemoryLogBuffer},
    service_control,
    settings::Settings,
    ui::{
        adapter_tab::render_adapter_inspector_tab, render_header, render_log_panel, render_tab_bar,
        route_tab::render_route_control_tab,
    },
    windows_net::{self, AdapterDetails},
};
use gpui_kit::{
    component::{
        input::InputState,
        select::{SearchableVec, SelectState},
        scroll::ScrollableElement,
        IndexPath,
    },
    prelude::FluentBuilder,
    *,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveTab {
    RouteControl,
    AdapterInspector,
}

pub struct RouteClientApp {
    pub active_tab: ActiveTab,
    pub settings: Settings,
    pub port_input: Entity<InputState>,
    pub interface_select: Entity<SelectState<SearchableVec<String>>>,
    pub interval_select: Entity<SelectState<SearchableVec<String>>>,
    pub interfaces: Vec<String>,
    pub adapter_details: Vec<AdapterDetails>,
    pub inspected_adapter: String,
    pub message: Option<String>,
    pub service_installed: bool,
    pub service_running: bool,
    pub service_status: String,
    pub logs: MemoryLogBuffer,
    pub last_board_ip: Option<String>,
    poll_tick: u64,
    _poll_task: Task<()>,
}

impl RouteClientApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut settings = Settings::load();
        let adapter_details = windows_net::inspect_all_adapters().unwrap_or_default();
        let interfaces = if !adapter_details.is_empty() {
            adapter_details
                .iter()
                .map(|a| a.friendly_name.clone())
                .collect()
        } else {
            windows_net::interface_names().unwrap_or_default()
        };

        if !interfaces
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&settings.interface))
        {
            if let Some(name) = interfaces
                .iter()
                .find(|name| name.eq_ignore_ascii_case("vEthernet (Main)"))
                .or_else(|| interfaces.first())
            {
                settings.interface = name.clone();
            }
        }
        let inspected_adapter = settings.interface.clone();

        let port_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("UDP port")
                .default_value(settings.discovery_port.to_string())
        });
        let interface_index = interfaces
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&settings.interface))
            .map(IndexPath::new);
        let interface_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(interfaces.clone()),
                interface_index,
                window,
                cx,
            )
            .searchable(true)
        });
        let intervals = ["1 秒", "3 秒", "5 秒", "10 秒", "30 秒"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let interval_index = intervals
            .iter()
            .position(|label| label == &format!("{} 秒", settings.interval_seconds))
            .map(IndexPath::new);
        let interval_select = cx
            .new(|cx| SelectState::new(SearchableVec::new(intervals), interval_index, window, cx));
        let poll_task = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            let keep_going = this
                .update(cx, |app, cx| {
                    app.poll_live_state(cx);
                })
                .is_ok();
            if !keep_going {
                break;
            }
        });
        let mut app = Self {
            active_tab: ActiveTab::RouteControl,
            settings,
            port_input,
            interface_select,
            interval_select,
            interfaces,
            adapter_details,
            inspected_adapter,
            message: None,
            service_installed: false,
            service_running: false,
            service_status: "正在查询服务状态…".into(),
            logs: MemoryLogBuffer::new(),
            last_board_ip: None,
            poll_tick: 0,
            _poll_task: poll_task,
        };
        app.refresh_service_status();
        app.sync_board_from_adapters();
        app.seed_initial_memory_logs();
        app
    }

    fn seed_initial_memory_logs(&mut self) {
        let up_count = self.adapter_details.iter().filter(|a| a.is_up).count();
        self.logs.push(
            LogLevel::Info,
            format!(
                "已读取系统网卡配置：共 {} 张网卡（{} 张处于连接状态）",
                self.adapter_details.len(),
                up_count
            ),
        );
        self.logs.push(
            LogLevel::Info,
            format!(
                "绑定监听接口 {} · 扫描策略 UDP :{} / {} 秒",
                self.settings.interface,
                self.settings.discovery_port,
                self.settings.interval_seconds
            ),
        );
        if let Some(board_ip) = &self.last_board_ip {
            self.logs.push(
                LogLevel::Normal,
                format!("检测到开发板路由已生效 · 下一跳节点 {board_ip}"),
            );
        } else if self.service_running {
            self.logs.push(
                LogLevel::Wait,
                format!(
                    "后台服务运行中 · 正在接口 {} 上探测开发板应答",
                    self.settings.interface
                ),
            );
        } else {
            self.logs.push(
                LogLevel::Wait,
                format!("后台服务状态：{} · 内存日志实时滚动就绪", self.service_status),
            );
        }
    }

    pub fn sync_board_from_adapters(&mut self) {
        let target = &self.settings.interface;
        self.last_board_ip = self
            .adapter_details
            .iter()
            .find(|a| a.friendly_name.eq_ignore_ascii_case(target))
            .and_then(|a| a.managed_board_ip)
            .or_else(|| {
                self.adapter_details
                    .iter()
                    .find_map(|a| a.managed_board_ip)
            })
            .map(|ip| ip.to_string());
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.persist_settings_from_ui(cx) {
            self.message = Some(error.clone());
            self.logs
                .push(LogLevel::Error, format!("保存配置校验失败：{error}"));
            cx.notify();
            return;
        }
        self.logs.push(
            LogLevel::Info,
            format!(
                "请求保存配置：接口 {} · UDP :{} · 周期 {} 秒",
                self.settings.interface,
                self.settings.discovery_port,
                self.settings.interval_seconds
            ),
        );
        self.request_elevated("save-config", true, cx);
    }

    fn persist_settings_from_ui(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let value = self.port_input.read(cx).value().to_string();
        let Ok(port) = value.parse::<u16>() else {
            return Err("UDP 端口必须是 1 到 65535 之间的整数".into());
        };
        if port == 0 {
            return Err("UDP 端口不能为 0".into());
        }
        let interface = self
            .interface_select
            .read(cx)
            .selected_value()
            .cloned()
            .ok_or_else(|| "请选择用于发现开发板的网卡".to_string())?;
        let interval = self
            .interval_select
            .read(cx)
            .selected_value()
            .and_then(|label| label.strip_suffix(" 秒"))
            .and_then(|seconds| seconds.parse::<u64>().ok())
            .ok_or_else(|| "请选择扫描间隔".to_string())?;
        self.settings.discovery_port = port;
        self.settings.interface = interface.clone();
        self.settings.interval_seconds = interval;
        self.inspected_adapter = interface;
        Ok(())
    }

    pub fn refresh_service_status(&mut self) {
        match service_control::status() {
            Ok(Some(state)) => {
                self.service_installed = true;
                self.service_running = state == windows_service::service::ServiceState::Running;
                self.service_status = format!("已安装 · {}", service_state_label(state));
            }
            Ok(None) => {
                self.service_installed = false;
                self.service_running = false;
                self.service_status = "未安装".into();
            }
            Err(error) => {
                self.service_installed = false;
                self.service_running = false;
                self.service_status = error;
            }
        }
    }

    pub fn install_service(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.persist_settings_from_ui(cx) {
            self.message = Some(error.clone());
            self.logs
                .push(LogLevel::Error, format!("安装服务前配置校验失败：{error}"));
            cx.notify();
            return;
        }
        self.logs.push(
            LogLevel::Info,
            "正在请求管理员权限以安装并启动后台服务…",
        );
        self.request_elevated("install", true, cx);
    }

    pub fn start_service(&mut self, cx: &mut Context<Self>) {
        self.logs
            .push(LogLevel::Info, "正在请求启动 EdgeLinkRouteService 后台服务…");
        self.request_elevated("start", false, cx);
    }

    pub fn stop_service(&mut self, cx: &mut Context<Self>) {
        self.logs
            .push(LogLevel::Wait, "正在请求停止 EdgeLinkRouteService 后台服务…");
        self.request_elevated("stop", false, cx);
    }

    pub fn uninstall_service(&mut self, cx: &mut Context<Self>) {
        self.logs
            .push(LogLevel::Wait, "正在请求卸载 EdgeLinkRouteService 后台服务…");
        self.request_elevated("uninstall", false, cx);
    }

    pub fn uninstall_application(&mut self, cx: &mut Context<Self>) {
        let candidates = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("uninstall.exe"))),
            std::env::var_os("ProgramW6432")
                .or_else(|| std::env::var_os("ProgramFiles"))
                .map(|p| PathBuf::from(p).join("EdgeLink").join("uninstall.exe")),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("edgelink-setup.exe"))),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("windows-route-setup.exe"))),
        ];
        let Some(uninstaller) = candidates.into_iter().flatten().find(|p| p.is_file()) else {
            self.message = Some("未找到卸载程序 (uninstall.exe)".into());
            self.logs
                .push(LogLevel::Error, "未找到卸载程序 (uninstall.exe)");
            cx.notify();
            return;
        };
        match std::process::Command::new(&uninstaller)
            .arg("--uninstall")
            .spawn()
        {
            Ok(_) => {
                self.logs
                    .push(LogLevel::Wait, "已启动应用卸载向导，请在弹出窗口中确认…");
                self.message = Some("已启动卸载向导，请在弹出的确认框中操作。".into());
            }
            Err(error) => {
                self.logs
                    .push(LogLevel::Error, format!("启动卸载程序失败：{error}"));
                self.message = Some(format!("启动卸载程序失败：{error}"));
            }
        }
        cx.notify();
    }

    fn request_elevated(
        &mut self,
        operation: &str,
        include_settings: bool,
        cx: &mut Context<Self>,
    ) {
        let result = if include_settings {
            write_temporary_settings(&self.settings).and_then(|path| {
                let parameters = format!("--elevated {operation} \"{}\"", path.display());
                elevation::launch(&parameters).map_err(|error| {
                    let _ = fs::remove_file(path);
                    error
                })
            })
        } else {
            elevation::launch(&format!("--elevated {operation}"))
        };
        self.message = Some(match result {
            Ok(()) => "请在 UAC 提示中确认；完成后状态将自动同步。".into(),
            Err(error) => {
                self.logs
                    .push(LogLevel::Error, format!("请求管理员权限失败：{error}"));
                format!("未能请求管理员权限：{error}")
            }
        });
        cx.notify();
    }

    pub fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.refresh_service_status();
        if let Ok(details) = windows_net::inspect_all_adapters() {
            self.adapter_details = details;
            self.sync_board_from_adapters();
        }
        self.logs.push(
            if self.service_running {
                LogLevel::Normal
            } else {
                LogLevel::Info
            },
            format!(
                "手动刷新完成 · 服务状态：{} · 绑定网卡：{}",
                self.service_status, self.settings.interface
            ),
        );
        self.message = Some("服务状态与网卡路由快照已刷新".into());
        cx.notify();
    }

    fn poll_live_state(&mut self, cx: &mut Context<Self>) {
        self.poll_tick = self.poll_tick.wrapping_add(1);
        let prev_status = self.service_status.clone();
        let prev_board = self.last_board_ip.clone();

        self.refresh_service_status();
        if let Ok(details) = windows_net::inspect_all_adapters() {
            self.adapter_details = details;
            self.sync_board_from_adapters();
        }

        let mut updated = false;

        if prev_status != self.service_status {
            let level = if self.service_running {
                LogLevel::Normal
            } else if self.service_installed {
                LogLevel::Wait
            } else {
                LogLevel::Info
            };
            self.logs.push(
                level,
                format!("后台服务状态更新为：{}", self.service_status),
            );
            updated = true;
        }

        if prev_board != self.last_board_ip {
            match &self.last_board_ip {
                Some(ip) => {
                    self.logs.push(
                        LogLevel::Normal,
                        format!(
                            "已发现开发板 {ip} · 接口 {} 路由表已同步更新",
                            self.settings.interface
                        ),
                    );
                }
                None => {
                    self.logs.push(
                        LogLevel::Wait,
                        format!(
                            "接口 {} 上的开发板接管路由已移除或等待应答",
                            self.settings.interface
                        ),
                    );
                }
            }
            updated = true;
        }

        if !updated && self.poll_tick % 2 == 0 {
            let selected_iface = self
                .interface_select
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_else(|| self.settings.interface.clone());
            let adapter_summary = self
                .adapter_details
                .iter()
                .find(|a| a.friendly_name.eq_ignore_ascii_case(&selected_iface));

            let (level, msg) = match (self.poll_tick / 2) % 3 {
                0 => {
                    if let Some(board_ip) = &self.last_board_ip {
                        (
                            LogLevel::Normal,
                            format!(
                                "内存巡检 · 开发板 {board_ip} 在线，接口 {selected_iface} 路由正常"
                            ),
                        )
                    } else if self.service_running {
                        (
                            LogLevel::Wait,
                            format!(
                                "内存巡检 · 正在通过 {selected_iface} (UDP :{}) 广播探测开发板…",
                                self.settings.discovery_port
                            ),
                        )
                    } else {
                        (
                            LogLevel::Info,
                            format!(
                                "内存巡检 · 服务状态：{}，监听接口：{selected_iface}",
                                self.service_status
                            ),
                        )
                    }
                }
                1 => {
                    if let Some(adapter) = adapter_summary {
                        let ip_text = adapter
                            .ipv4_addresses
                            .first()
                            .map(|ip| format!("{}/{}", ip.address, ip.prefix_length))
                            .unwrap_or_else(|| "无 IPv4".to_owned());
                        let dhcp_text = if adapter.dhcp_enabled {
                            "DHCP"
                        } else {
                            "静态 IP"
                        };
                        (
                            if adapter.is_up {
                                LogLevel::Normal
                            } else {
                                LogLevel::Wait
                            },
                            format!(
                                "网卡状态读取 · {} [{}] IPv4: {} ({})",
                                adapter.friendly_name,
                                adapter.oper_status_label,
                                ip_text,
                                dhcp_text
                            ),
                        )
                    } else {
                        (
                            LogLevel::Wait,
                            format!("网卡状态读取 · 未找到接口 {selected_iface}"),
                        )
                    }
                }
                _ => {
                    let route_count = adapter_summary.map(|a| a.active_routes.len()).unwrap_or(0);
                    (
                        LogLevel::Info,
                        format!(
                            "路由表快照 · 接口 {selected_iface} 当前活动 IPv4 路由共 {route_count} 条"
                        ),
                    )
                }
            };
            self.logs.push(level, msg);
            updated = true;
        }

        if updated {
            cx.notify();
        }
    }

    pub fn refresh_interfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match windows_net::inspect_all_adapters() {
            Ok(details) => {
                let interfaces: Vec<String> =
                    details.iter().map(|a| a.friendly_name.clone()).collect();
                let previous = self
                    .interface_select
                    .read(cx)
                    .selected_value()
                    .cloned()
                    .unwrap_or_else(|| self.settings.interface.clone());
                let selected = interfaces
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case(&previous))
                    .cloned()
                    .or_else(|| interfaces.first().cloned());
                self.interface_select.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(interfaces.clone()), window, cx);
                    if let Some(selected) = &selected {
                        state.set_selected_value(selected, window, cx);
                    }
                });
                if let Some(selected) = selected {
                    self.settings.interface = selected.clone();
                    if !interfaces
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(&self.inspected_adapter))
                    {
                        self.inspected_adapter = selected;
                    }
                }
                self.interfaces = interfaces;
                self.adapter_details = details;
                self.sync_board_from_adapters();
                self.logs.push(
                    LogLevel::Info,
                    format!(
                        "已重新读取系统网卡信息：共 {} 张网卡",
                        self.adapter_details.len()
                    ),
                );
                self.message = Some("网卡列表及配置信息已刷新".into());
            }
            Err(error) => {
                self.logs
                    .push(LogLevel::Error, format!("读取网卡配置失败：{error}"));
                self.message = Some(format!("读取网卡失败：{error}"));
            }
        }
        cx.notify();
    }

    pub fn select_inspected_adapter(&mut self, name: String, cx: &mut Context<Self>) {
        self.inspected_adapter = name.clone();
        if let Some(adapter) = self
            .adapter_details
            .iter()
            .find(|a| a.friendly_name.eq_ignore_ascii_case(&name))
            .cloned()
        {
            let ip_summary = adapter
                .ipv4_addresses
                .first()
                .map(|e| format!("{}/{}", e.address, e.prefix_length))
                .unwrap_or_else(|| "无 IPv4 地址".to_owned());
            self.logs.push(
                LogLevel::Info,
                format!(
                    "读取网卡 {} (#{}) · {} · IPv4: {}",
                    adapter.friendly_name,
                    adapter.if_index,
                    adapter.oper_status_label,
                    ip_summary
                ),
            );
        }
        cx.notify();
    }

    pub fn bind_inspected_adapter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.inspected_adapter.clone();
        if target.is_empty() {
            return;
        }
        self.interface_select.update(cx, |state, cx| {
            state.set_selected_value(&target, window, cx);
        });
        self.settings.interface = target.clone();
        self.sync_board_from_adapters();
        self.logs.push(
            LogLevel::Normal,
            format!("已将网卡 {target} 设为当前绑定接口（点击「保存配置」可写入服务配置）"),
        );
        self.message = Some(format!(
            "已切换绑定网卡为「{target}」，请切回路由控制页保存配置使其对后台服务生效。"
        ));
        cx.notify();
    }
}

impl Render for RouteClientApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let installed = self.service_installed;
        let running = self.service_running;
        let selected_iface = self
            .interface_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.settings.interface.clone());
        let selected_interval = self
            .interval_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| format!("{} 秒", self.settings.interval_seconds));
        let current_port = self.port_input.read(cx).value().to_string();
        let board_label = self
            .last_board_ip
            .clone()
            .unwrap_or_else(|| "等待发现…".to_string());
        let has_board = self.last_board_ip.is_some();
        let active_tab = self.active_tab;
        let adapter_count = self.adapter_details.len();

        div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scrollbar()
            .bg(rgb(0x0a0a0a))
            .text_color(rgb(0xfafafa))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .gap_3()
                    .px_6()
                    .py_4()
                    .child(render_header(installed, running))
                    .child(render_tab_bar(
                        active_tab,
                        adapter_count,
                        &selected_iface,
                        cx,
                    ))
                    .when(active_tab == ActiveTab::RouteControl, |el| {
                        el.child(render_route_control_tab(
                            self,
                            installed,
                            running,
                            &selected_iface,
                            &selected_interval,
                            &current_port,
                            &board_label,
                            has_board,
                            cx,
                        ))
                    })
                    .when(active_tab == ActiveTab::AdapterInspector, |el| {
                        el.child(render_adapter_inspector_tab(self, &selected_iface, cx))
                    })
                    .child(render_log_panel(&self.logs))
                    .when_some(self.message.clone(), |element, message| {
                        element.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .w_full()
                                .rounded_xl()
                                .bg(rgb(0x141417))
                                .border_1()
                                .border_color(rgb(0x27272a))
                                .px_4()
                                .py_2()
                                .text_size(px(12.))
                                .text_color(rgb(0xe4e4e7))
                                .child(format!("提示：{message}")),
                        )
                    }),
            )
    }
}

fn service_state_label(state: windows_service::service::ServiceState) -> &'static str {
    use windows_service::service::ServiceState;
    match state {
        ServiceState::Stopped => "已停止",
        ServiceState::StartPending => "正在启动",
        ServiceState::StopPending => "正在停止",
        ServiceState::Running => "运行中",
        ServiceState::ContinuePending => "正在恢复",
        ServiceState::PausePending => "正在暂停",
        ServiceState::Paused => "已暂停",
    }
}

fn write_temporary_settings(settings: &Settings) -> Result<PathBuf, String> {
    let bytes = serde_json::to_vec(settings).map_err(|error| format!("无法准备配置：{error}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "edgelink-{}-{stamp}.json",
        std::process::id()
    ));
    fs::write(&path, bytes).map_err(|error| format!("无法暂存配置：{error}"))?;
    Ok(path)
}
