pub mod adapter_tab;
pub mod route_tab;

use crate::{
    app::{ActiveTab, RouteClientApp},
    memory_log::{LogEntry, LogLevel, MemoryLogBuffer},
    windows_net,
};
use gpui_kit::{prelude::FluentBuilder, *};

pub fn render_header(installed: bool, running: bool) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .gap_4()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3p5()
                .min_w_0()
                .child(app_brand_icon())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .rounded_md()
                                        .bg(rgb(0x18181b))
                                        .border_1()
                                        .border_color(rgb(0x27272a))
                                        .px_2()
                                        .py_0p5()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0x38bdf8))
                                        .child("EDGELINK"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(0x71717a))
                                        .child("Windows 路由助手"),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(21.))
                                .font_weight(FontWeight::BOLD)
                                .child("EdgeLink 路由助手"),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(0xa1a1aa))
                                .child("自动发现边缘 Linux 开发板（EdgeLink Beacon），实时维护 Windows 路由表与网卡配置读取"),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_full()
                        .border_1()
                        .border_color(if running {
                            rgb(0x14532d)
                        } else if installed {
                            rgb(0x713f12)
                        } else {
                            rgb(0x27272a)
                        })
                        .bg(if running {
                            rgb(0x052e16)
                        } else if installed {
                            rgb(0x271705)
                        } else {
                            rgb(0x18181b)
                        })
                        .px_3p5()
                        .py_1()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if running {
                            rgb(0x4ade80)
                        } else if installed {
                            rgb(0xfacc15)
                        } else {
                            rgb(0xa1a1aa)
                        })
                        .child(if running {
                            "● 服务运行中"
                        } else if installed {
                            "● 服务已停止"
                        } else {
                            "● 服务未安装"
                        }),
                ),
        )
}

pub fn render_tab_bar(
    active_tab: ActiveTab,
    adapter_count: usize,
    selected_iface: &str,
    cx: &mut Context<RouteClientApp>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .pb_2()
        .border_b_1()
        .border_color(rgb(0x262626))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .p_1()
                .rounded_lg()
                .bg(rgb(0x111113))
                .border_1()
                .border_color(rgb(0x262626))
                .child(
                    div()
                        .id("tab-route-control")
                        .cursor_pointer()
                        .rounded_md()
                        .px_3p5()
                        .py_1()
                        .bg(if active_tab == ActiveTab::RouteControl {
                            rgb(0x27272a)
                        } else {
                            rgb(0x111113)
                        })
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if active_tab == ActiveTab::RouteControl {
                            rgb(0xfafafa)
                        } else {
                            rgb(0xa1a1aa)
                        })
                        .child("路由与服务控制")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.active_tab != ActiveTab::RouteControl {
                                this.active_tab = ActiveTab::RouteControl;
                                this.logs.push(
                                    LogLevel::Info,
                                    "已切换至「路由与服务控制」标签页",
                                );
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .id("tab-adapter-inspector")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .px_3p5()
                        .py_1()
                        .bg(if active_tab == ActiveTab::AdapterInspector {
                            rgb(0x27272a)
                        } else {
                            rgb(0x111113)
                        })
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if active_tab == ActiveTab::AdapterInspector {
                            rgb(0xfafafa)
                        } else {
                            rgb(0xa1a1aa)
                        })
                        .child("网卡信息配置读取")
                        .child(
                            div()
                                .rounded_full()
                                .bg(rgb(0x18181b))
                                .border_1()
                                .border_color(rgb(0x3f3f46))
                                .px_2()
                                .py_0p5()
                                .text_size(px(11.))
                                .text_color(rgb(0x38bdf8))
                                .child(format!("{adapter_count}")),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.active_tab != ActiveTab::AdapterInspector {
                                this.active_tab = ActiveTab::AdapterInspector;
                                if let Ok(details) = windows_net::inspect_all_adapters() {
                                    this.adapter_details = details;
                                    this.sync_board_from_adapters();
                                }
                                this.logs.push(
                                    LogLevel::Info,
                                    format!(
                                        "已切换至「网卡信息配置读取」· 当前查看 {}",
                                        this.inspected_adapter
                                    ),
                                );
                                cx.notify();
                            }
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .text_color(rgb(0xa1a1aa))
                .child(format!("当前绑定网卡：{selected_iface}")),
        )
}

pub fn render_log_panel(logs: &MemoryLogBuffer) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap_2()
        .rounded_xl()
        .bg(rgb(0x111113))
        .border_1()
        .border_color(rgb(0x262626))
        .px_4()
        .py_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::BOLD)
                                .child("实时运行日志"),
                        )
                        .child(
                            div()
                                .rounded_full()
                                .bg(rgb(0x18181b))
                                .border_1()
                                .border_color(rgb(0x27272a))
                                .px_2()
                                .py_0p5()
                                .text_size(px(10.))
                                .text_color(rgb(0xa1a1aa))
                                .child("内存实时滚动更新 · 最新 3 条"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x71717a))
                        .child("仅驻留内存 · 无磁盘日志文件"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .children(
                    logs.iter()
                        .enumerate()
                        .map(|(index, entry)| render_log_row(index == 0, entry)),
                ),
        )
}

pub fn render_log_row(is_latest: bool, entry: &LogEntry) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .gap_3()
        .rounded_lg()
        .bg(if is_latest {
            rgb(0x18181b)
        } else {
            rgb(0x0a0a0a)
        })
        .border_1()
        .border_color(if is_latest {
            rgb(0x3f3f46)
        } else {
            rgb(0x222225)
        })
        .px_3()
        .py_1p5()
        .child(
            div()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .gap_2p5()
                .child(
                    div()
                        .rounded_md()
                        .bg(rgb(entry.badge_bg))
                        .border_1()
                        .border_color(rgb(entry.badge_border))
                        .px_2()
                        .py_0p5()
                        .text_size(px(10.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(entry.level_color))
                        .child(entry.level_label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_size(px(12.))
                        .font_weight(if is_latest {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if is_latest {
                            rgb(0xfafafa)
                        } else {
                            rgb(0xd4d4d8)
                        })
                        .child(entry.message.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .when(is_latest, |el| {
                    el.child(
                        div()
                            .rounded_sm()
                            .bg(rgb(0x27272a))
                            .px_1p5()
                            .py_0p5()
                            .text_size(px(10.))
                            .text_color(rgb(0xe4e4e7))
                            .child("NEW"),
                    )
                })
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x71717a))
                        .child(entry.timestamp.clone()),
                ),
        )
}

pub fn app_brand_icon() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(46.))
        .rounded_xl()
        .bg(rgb(0x0f172a))
        .border_1()
        .border_color(rgb(0x1e3a5f))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .size(px(10.))
                        .rounded_full()
                        .bg(rgb(0x38bdf8))
                        .border_2()
                        .border_color(rgb(0xe0f2fe)),
                )
                .child(
                    div()
                        .w(px(7.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(rgb(0x38bdf8)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .size(px(9.))
                                .rounded_full()
                                .bg(rgb(0x10b981))
                                .border_2()
                                .border_color(rgb(0xd1fae5)),
                        )
                        .child(
                            div()
                                .size(px(8.))
                                .rounded_full()
                                .bg(rgb(0x3b82f6))
                                .border_2()
                                .border_color(rgb(0xdbeafe)),
                        ),
                ),
        )
}

pub fn summary_metric_card(
    title: &str,
    value: &str,
    subtitle: &str,
    accent_hex: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .gap_1()
        .rounded_xl()
        .bg(rgb(0x111113))
        .border_1()
        .border_color(rgb(0x262626))
        .px_4()
        .py_3()
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0xa1a1aa))
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(accent_hex))
                .child(value.to_owned()),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(0x71717a))
                .child(subtitle.to_owned()),
        )
}

pub fn detail_kv_box(label: &str, value: &str, value_color: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .gap_1()
        .rounded_lg()
        .bg(rgb(0x0a0a0a))
        .border_1()
        .border_color(rgb(0x222225))
        .px_3()
        .py_2()
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(0x71717a))
                .child(label.to_owned()),
        )
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(value_color))
                .child(value.to_owned()),
        )
}
