use crate::{app::RouteClientApp, ui::summary_metric_card};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants as _},
        input::Input,
        select::Select,
        Sizable,
    },
    prelude::FluentBuilder,
    *,
};

#[allow(clippy::too_many_arguments)]
pub fn render_route_control_tab(
    app: &RouteClientApp,
    installed: bool,
    running: bool,
    selected_iface: &str,
    selected_interval: &str,
    current_port: &str,
    board_label: &str,
    has_board: bool,
    cx: &mut Context<RouteClientApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap_3p5()
        // 关键指标概览条 (4 列)
        .child(
            div()
                .flex()
                .w_full()
                .gap_3()
                .child(summary_metric_card(
                    "监听网卡",
                    selected_iface,
                    "绑定接口",
                    0xf4f4f5,
                ))
                .child(summary_metric_card(
                    "扫描策略",
                    &format!("UDP :{} · {}", current_port, selected_interval),
                    "广播探测",
                    0xe4e4e7,
                ))
                .child(summary_metric_card(
                    "最近发现开发板",
                    board_label,
                    if has_board {
                        "路由已指向该节点"
                    } else {
                        "尚未收到开发板应答"
                    },
                    if has_board { 0x4ade80 } else { 0xa1a1aa },
                ))
                .child(summary_metric_card(
                    "服务状态",
                    &app.service_status,
                    "EdgeLinkRouteService",
                    if running {
                        0x4ade80
                    } else if installed {
                        0xfacc15
                    } else {
                        0xa1a1aa
                    },
                )),
        )
        // 主体双栏区域：左侧参数配置 + 右侧服务控制与操作
        .child(
            div()
                .flex()
                .w_full()
                .gap_3p5()
                .child(
                    // 左栏：网络接口与发现参数配置
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap_4()
                        .rounded_xl()
                        .bg(rgb(0x111113))
                        .border_1()
                        .border_color(rgb(0x262626))
                        .p_5()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_size(px(16.))
                                                .font_weight(FontWeight::BOLD)
                                                .child("网络与发现参数"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(0xa1a1aa))
                                                .child("配置用于局域网探测开发板的网卡、周期与 UDP 端口"),
                                        ),
                                )
                                .child(
                                    Button::new("refresh-interfaces")
                                        .secondary()
                                        .with_size(gpui_kit::component::Size::Small)
                                        .label("刷新网卡")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.refresh_interfaces(window, cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0xd4d4d8))
                                        .child("绑定网络接口"),
                                )
                                .child(
                                    Select::new(&app.interface_select)
                                        .with_size(gpui_kit::component::Size::Small)
                                        .w_full()
                                        .placeholder("请选择网卡")
                                        .accessibility_label("用于发现开发板的网卡"),
                                )
                                .when(app.interfaces.is_empty(), |element| {
                                    element.child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0xf87171))
                                            .child("未读取到可用网卡，请点击右上角「刷新网卡」。"),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0xd4d4d8))
                                                .child("扫描间隔"),
                                        )
                                        .child(
                                            Select::new(&app.interval_select)
                                                .with_size(gpui_kit::component::Size::Small)
                                                .w_full()
                                                .placeholder("请选择扫描间隔")
                                                .accessibility_label("扫描间隔"),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0xd4d4d8))
                                                .child("发现端口 (UDP)"),
                                        )
                                        .child(
                                            Input::new(&app.port_input)
                                                .with_size(gpui_kit::component::Size::Small)
                                                .w_full(),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .pt_2()
                                .border_t_1()
                                .border_color(rgb(0x262626))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(0x71717a))
                                        .child("保存后将在下一扫描周期对后台服务生效"),
                                )
                                .child(
                                    Button::new("save-settings")
                                        .primary()
                                        .with_size(gpui_kit::component::Size::Small)
                                        .label("保存配置")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.save_settings(cx)
                                        })),
                                ),
                        ),
                )
                .child(
                    // 右栏：Windows 后台服务生命周期管理与说明
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .justify_between()
                        .gap_4()
                        .rounded_xl()
                        .bg(rgb(0x111113))
                        .border_1()
                        .border_color(rgb(0x262626))
                        .p_5()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_0p5()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .text_size(px(16.))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child("后台服务控制"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .text_color(rgb(0xa1a1aa))
                                                        .child("管理 Windows 系统级守护进程与路由自动回退"),
                                                ),
                                        )
                                        .child(
                                            Button::new("refresh-status")
                                                .secondary()
                                                .with_size(gpui_kit::component::Size::Small)
                                                .label("刷新状态")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.refresh_status(cx)
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1p5()
                                        .rounded_lg()
                                        .bg(rgb(0x0a0a0a))
                                        .border_1()
                                        .border_color(rgb(0x222225))
                                        .p_3p5()
                                        .text_size(px(12.))
                                        .text_color(rgb(0xa1a1aa))
                                        .child("• 发现开发板后自动维护 0.0.0.0/1 与 128.0.0.0/1 高优先级路由")
                                        .child("• 连续 3 个周期未响应或服务停止时，自动移除路由并恢复网卡 DHCP")
                                        .child("• 关闭本窗口后程序驻留系统托盘，随时可双击托盘图标唤出"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2p5()
                                .pt_2()
                                .border_t_1()
                                .border_color(rgb(0x262626))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(0x71717a))
                                        .child("服务操作（需要 UAC 管理员权限确认）"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2p5()
                                        .when(!installed, |element| {
                                            element.child(
                                                Button::new("install-service")
                                                    .primary()
                                                    .with_size(gpui_kit::component::Size::Small)
                                                    .label("安装并启动服务")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.install_service(cx)
                                                    })),
                                            )
                                        })
                                        .when(installed && !running, |element| {
                                            element.child(
                                                Button::new("start-service")
                                                    .primary()
                                                    .with_size(gpui_kit::component::Size::Small)
                                                    .label("启动服务")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.start_service(cx)
                                                    })),
                                            )
                                        })
                                        .when(running, |element| {
                                            element.child(
                                                Button::new("stop-service")
                                                    .secondary()
                                                    .with_size(gpui_kit::component::Size::Small)
                                                    .label("停止服务")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.stop_service(cx)
                                                    })),
                                            )
                                        })
                                        .when(installed, |element| {
                                            element.child(
                                                Button::new("uninstall-service")
                                                    .danger()
                                                    .with_size(gpui_kit::component::Size::Small)
                                                    .label("卸载服务")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.uninstall_service(cx)
                                                    })),
                                            )
                                        })
                                        .child(
                                            Button::new("uninstall-application")
                                                .danger()
                                                .with_size(gpui_kit::component::Size::Small)
                                                .label("卸载客户端应用")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.uninstall_application(cx)
                                                })),
                                        ),
                                ),
                        ),
                ),
        )
}
