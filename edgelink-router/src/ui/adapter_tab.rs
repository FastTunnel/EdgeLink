use crate::{app::RouteClientApp, ui::detail_kv_box};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants as _},
        scroll::ScrollableElement,
        Sizable,
    },
    prelude::FluentBuilder,
    *,
};

pub fn render_adapter_inspector_tab(
    app: &RouteClientApp,
    bound_iface: &str,
    cx: &mut Context<RouteClientApp>,
) -> impl IntoElement {
    let current_adapter = app
        .adapter_details
        .iter()
        .find(|a| a.friendly_name.eq_ignore_ascii_case(&app.inspected_adapter))
        .or_else(|| app.adapter_details.first())
        .cloned();

    let is_bound = current_adapter
        .as_ref()
        .is_some_and(|a| a.friendly_name.eq_ignore_ascii_case(bound_iface));

    div()
        .flex()
        .w_full()
        .h(px(485.))
        .min_h_0()
        .gap_3p5()
        // 左侧：系统网卡列表选择栏（固定高度 + 独立纵向滚动，完整展示所有网卡）
        .child(
            div()
                .flex()
                .flex_col()
                .w(px(270.))
                .h_full()
                .min_h_0()
                .gap_2p5()
                .rounded_xl()
                .bg(rgb(0x111113))
                .border_1()
                .border_color(rgb(0x262626))
                .p_3p5()
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
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "系统网卡列表 ({})",
                                            app.adapter_details.len()
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(0xa1a1aa))
                                        .child("点击网卡读取完整配置"),
                                ),
                        )
                        .child(
                            Button::new("reload-adapters-tab")
                                .secondary()
                                .with_size(gpui_kit::component::Size::Small)
                                .label("读取刷新")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.refresh_interfaces(window, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .id("adapter-list-scroll")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .pr_1()
                        .gap_1p5()
                        .children(app.adapter_details.iter().enumerate().map(
                            |(idx, adapter)| {
                                let is_selected = current_adapter.as_ref().is_some_and(|cur| {
                                    cur.friendly_name
                                        .eq_ignore_ascii_case(&adapter.friendly_name)
                                });
                                let is_this_bound =
                                    adapter.friendly_name.eq_ignore_ascii_case(bound_iface);
                                let primary_ip = adapter
                                    .ipv4_addresses
                                    .first()
                                    .map(|e| format!("{}/{}", e.address, e.prefix_length))
                                    .unwrap_or_else(|| {
                                        if adapter.ipv4_unbound_or_bridged {
                                            "桥接 / 未绑定 IPv4".to_owned()
                                        } else {
                                            "无 IPv4 地址".to_owned()
                                        }
                                    });
                                let name_for_click = adapter.friendly_name.clone();

                                div()
                                    .id(("adapter-item", idx))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .gap_1()
                                    .rounded_lg()
                                    .bg(if is_selected {
                                        rgb(0x18181b)
                                    } else {
                                        rgb(0x0a0a0a)
                                    })
                                    .border_1()
                                    .border_color(if is_selected {
                                        rgb(0x38bdf8)
                                    } else {
                                        rgb(0x222225)
                                    })
                                    .px_2p5()
                                    .py_2()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_inspected_adapter(name_for_click.clone(), cx);
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .w_full()
                                            .min_w_0()
                                            .gap_1p5()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .gap_1p5()
                                                    .child(
                                                        div()
                                                            .text_size(px(10.))
                                                            .text_color(if adapter.is_up {
                                                                rgb(0x4ade80)
                                                            } else {
                                                                rgb(0x71717a)
                                                            })
                                                            .child("●"),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .overflow_hidden()
                                                            .text_size(px(12.))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(if is_selected {
                                                                rgb(0xfafafa)
                                                            } else {
                                                                rgb(0xd4d4d8)
                                                            })
                                                            .child(adapter.friendly_name.clone()),
                                                    ),
                                            )
                                            .when(is_this_bound, |el| {
                                                el.child(
                                                    div()
                                                        .rounded_sm()
                                                        .bg(rgb(0x052e16))
                                                        .border_1()
                                                        .border_color(rgb(0x14532d))
                                                        .px_1p5()
                                                        .py_0p5()
                                                        .text_size(px(10.))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(rgb(0x4ade80))
                                                        .child("已绑定"),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .w_full()
                                            .text_size(px(11.))
                                            .text_color(rgb(0xa1a1aa))
                                            .child(primary_ip)
                                            .child(format!("IfIndex #{}", adapter.if_index)),
                                    )
                            },
                        )),
                ),
        )
        // 右侧：选中网卡完整详细配置面板（严格限制最小宽度 min_w_0，内部可纵向滚动显示全部项）
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .h_full()
                .min_h_0()
                .overflow_hidden()
                .gap_3()
                .rounded_xl()
                .bg(rgb(0x111113))
                .border_1()
                .border_color(rgb(0x262626))
                .p_4()
                .when_some(current_adapter, |el, adapter| {
                    let gw_text = if adapter.gateways.is_empty() {
                        "未配置默认网关".to_owned()
                    } else {
                        adapter
                            .gateways
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let dns_text = if adapter.dns_servers.is_empty() {
                        "未配置 IPv4 DNS".to_owned()
                    } else {
                        adapter
                            .dns_servers
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let dhcp_srv_text = adapter
                        .dhcp_server
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| "无 / 静态分配".to_owned());
                    let board_route_text = adapter
                        .managed_board_ip
                        .map(|ip| format!("已接管 -> {ip}"))
                        .unwrap_or_else(|| "未接管开发板路由".to_owned());
                    let dns_suffix_text = if adapter.dns_suffix.is_empty() {
                        "无".to_owned()
                    } else {
                        adapter.dns_suffix.clone()
                    };

                    el.child(
                        // 顶部网卡标题与快捷绑定按钮（不放过长 GUID，彻底避免横向撑爆）
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .min_w_0()
                            .gap_3()
                            .pb_2p5()
                            .border_b_1()
                            .border_color(rgb(0x262626))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(17.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(adapter.friendly_name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .rounded_full()
                                                    .bg(if adapter.is_up {
                                                        rgb(0x052e16)
                                                    } else {
                                                        rgb(0x18181b)
                                                    })
                                                    .border_1()
                                                    .border_color(if adapter.is_up {
                                                        rgb(0x14532d)
                                                    } else {
                                                        rgb(0x27272a)
                                                    })
                                                    .px_2p5()
                                                    .py_0p5()
                                                    .text_size(px(11.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(if adapter.is_up {
                                                        rgb(0x4ade80)
                                                    } else {
                                                        rgb(0xa1a1aa)
                                                    })
                                                    .child(adapter.oper_status_label.clone()),
                                            )
                                            .child(
                                                div()
                                                    .rounded_md()
                                                    .bg(rgb(0x18181b))
                                                    .border_1()
                                                    .border_color(rgb(0x27272a))
                                                    .px_2()
                                                    .py_0p5()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(0xa1a1aa))
                                                    .child(format!("接口索引 #{}", adapter.if_index)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(0xa1a1aa))
                                            .child(format!("硬件描述：{}", adapter.description)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .when(!is_bound, |btn_wrap| {
                                        btn_wrap.child(
                                            Button::new("bind-this-adapter")
                                                .primary()
                                                .with_size(gpui_kit::component::Size::Small)
                                                .label("设为路由绑定网卡")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.bind_inspected_adapter(window, cx)
                                                })),
                                        )
                                    })
                                    .when(is_bound, |btn_wrap| {
                                        btn_wrap.child(
                                            div()
                                                .rounded_md()
                                                .bg(rgb(0x18181b))
                                                .border_1()
                                                .border_color(rgb(0x3f3f46))
                                                .px_3()
                                                .py_1()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0x38bdf8))
                                                .child("✓ 当前绑定网卡"),
                                        )
                                    }),
                            ),
                    )
                    // 详情内容滚动区（确保所有字段与全部路由条目都能完整查看）
                    .child(
                        div()
                            .id("adapter-detail-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .pr_1()
                            .gap_2p5()
                            // 第一行参数网格（物理地址 / 类型与 MTU / 链路速率）
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2p5()
                                    .child(detail_kv_box(
                                        "物理地址 (MAC)",
                                        &adapter.mac_address,
                                        0xf4f4f5,
                                    ))
                                    .child(detail_kv_box(
                                        "接口类型 / MTU",
                                        &format!("{} · {} B", adapter.if_type_label, adapter.mtu),
                                        0xe4e4e7,
                                    ))
                                    .child(detail_kv_box(
                                        "链路速率 (上行 / 下行)",
                                        &format!(
                                            "{} / {}",
                                            adapter.tx_speed_label, adapter.rx_speed_label
                                        ),
                                        0xe4e4e7,
                                    )),
                            )
                            // 第二行参数网格（DHCP 模式 / DHCP 服务器 / 开发板接管路由）
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2p5()
                                    .child(detail_kv_box(
                                        "IPv4 地址分配模式",
                                        if adapter.ipv4_unbound_or_bridged {
                                            "桥接 / 未绑定 TCP/IPv4"
                                        } else if adapter.dhcp_enabled {
                                            "DHCP 动态获取"
                                        } else {
                                            "静态 IPv4 配置"
                                        },
                                        if adapter.ipv4_unbound_or_bridged {
                                            0xa1a1aa
                                        } else if adapter.dhcp_enabled {
                                            0x38bdf8
                                        } else {
                                            0xfacc15
                                        },
                                    ))
                                    .child(detail_kv_box("DHCP 服务器", &dhcp_srv_text, 0xd4d4d8))
                                    .child(detail_kv_box(
                                        "开发板接管路由",
                                        &board_route_text,
                                        if adapter.managed_board_ip.is_some() {
                                            0x4ade80
                                        } else {
                                            0xa1a1aa
                                        },
                                    )),
                            )
                            // 第三行参数网格（默认网关 / DNS 服务器 / 跃点数与标识）
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2p5()
                                    .child(detail_kv_box("默认网关", &gw_text, 0xf4f4f5))
                                    .child(detail_kv_box("DNS 服务器", &dns_text, 0xf4f4f5))
                                    .child(detail_kv_box(
                                        "接口跃点数 / DNS 后缀",
                                        &format!(
                                            "Metric {} · 后缀 {}",
                                            adapter.ipv4_metric, dns_suffix_text
                                        ),
                                        0xd4d4d8,
                                    )),
                            )
                            // 适配器 GUID 独立展示行
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .min_w_0()
                                    .rounded_lg()
                                    .bg(rgb(0x0a0a0a))
                                    .border_1()
                                    .border_color(rgb(0x222225))
                                    .px_3()
                                    .py_1p5()
                                    .text_size(px(11.))
                                    .child(
                                        div()
                                            .text_color(rgb(0x71717a))
                                            .child("适配器设备标识 (Adapter GUID)"),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(0xa1a1aa))
                                            .child(adapter.adapter_guid.clone()),
                                    ),
                            )
                            // IPv4 单播地址列表（全部展示）
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .min_w_0()
                                    .gap_1p5()
                                    .rounded_lg()
                                    .bg(rgb(0x0a0a0a))
                                    .border_1()
                                    .border_color(rgb(0x222225))
                                    .p_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0xf4f4f5))
                                                    .child("IPv4 单播地址列表"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(0x71717a))
                                                    .child(format!(
                                                        "共 {} 个地址",
                                                        adapter.ipv4_addresses.len()
                                                    )),
                                            ),
                                    )
                                    .when(adapter.ipv4_addresses.is_empty(), |box_el| {
                                        box_el.child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(0xa1a1aa))
                                                .child(if adapter.ipv4_unbound_or_bridged {
                                                    "该网卡未直接勾选「TCP/IPv4」协议（已作为底层桥接端口，IPv4 地址由上层「网桥 / vEthernet」承载）"
                                                } else {
                                                    "该网卡当前未绑定任何 IPv4 单播地址"
                                                }),
                                        )
                                    })
                                    .children(adapter.ipv4_addresses.iter().map(|ip_entry| {
                                        let bcast = ip_entry
                                            .broadcast
                                            .map(|b| b.to_string())
                                            .unwrap_or_else(|| "-".to_owned());
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .w_full()
                                            .min_w_0()
                                            .rounded_md()
                                            .bg(rgb(0x111113))
                                            .border_1()
                                            .border_color(rgb(0x262626))
                                            .px_3()
                                            .py_1p5()
                                            .text_size(px(12.))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2p5()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0x38bdf8))
                                                            .child(format!(
                                                                "{}/{}",
                                                                ip_entry.address,
                                                                ip_entry.prefix_length
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0xa1a1aa))
                                                            .child(format!(
                                                                "子网掩码 {}",
                                                                ip_entry.netmask
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0x71717a))
                                                            .child(format!("广播地址 {bcast}")),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(if ip_entry.preferred {
                                                        rgb(0x4ade80)
                                                    } else {
                                                        rgb(0xfacc15)
                                                    })
                                                    .child(if ip_entry.preferred {
                                                        "首选地址 (Preferred)"
                                                    } else {
                                                        "待确认地址"
                                                    }),
                                            )
                                    })),
                            )
                            // 接口关联 IPv4 路由表（展示全部路由项，不再截断为 4 条）
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .min_w_0()
                                    .gap_1p5()
                                    .rounded_lg()
                                    .bg(rgb(0x0a0a0a))
                                    .border_1()
                                    .border_color(rgb(0x222225))
                                    .p_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0xf4f4f5))
                                                    .child("接口关联 IPv4 路由表（完整列表）"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(0x71717a))
                                                    .child(format!(
                                                        "共 {} 条路由项",
                                                        adapter.active_routes.len()
                                                    )),
                                            ),
                                    )
                                    .when(adapter.active_routes.is_empty(), |r_el| {
                                        r_el.child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(0x71717a))
                                                .child("当前网卡无活动 IPv4 路由项"),
                                        )
                                    })
                                    .children(adapter.active_routes.iter().map(|route| {
                                        let route_kind = if route.is_managed_board_route {
                                            "开发板接管路由"
                                        } else if route.destination == "0.0.0.0/0" {
                                            "默认网关路由"
                                        } else if route.next_hop.is_unspecified() {
                                            "在链路上 (直连)"
                                        } else {
                                            "静态/系统路由"
                                        };
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .w_full()
                                            .min_w_0()
                                            .rounded_md()
                                            .bg(rgb(0x111113))
                                            .border_1()
                                            .border_color(if route.is_managed_board_route {
                                                rgb(0x14532d)
                                            } else {
                                                rgb(0x222225)
                                            })
                                            .px_3()
                                            .py_1()
                                            .text_size(px(12.))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(
                                                                if route.is_managed_board_route {
                                                                    rgb(0x4ade80)
                                                                } else {
                                                                    rgb(0xe4e4e7)
                                                                },
                                                            )
                                                            .child(format!(
                                                                "目标 {}",
                                                                route.destination
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0xa1a1aa))
                                                            .child(format!(
                                                                "下一跳 {}",
                                                                route.next_hop
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .rounded_sm()
                                                            .bg(if route.is_managed_board_route {
                                                                rgb(0x052e16)
                                                            } else {
                                                                rgb(0x18181b)
                                                            })
                                                            .px_1p5()
                                                            .py_0p5()
                                                            .text_size(px(10.))
                                                            .text_color(
                                                                if route.is_managed_board_route {
                                                                    rgb(0x4ade80)
                                                                } else {
                                                                    rgb(0xa1a1aa)
                                                                },
                                                            )
                                                            .child(route_kind),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(0x71717a))
                                                            .child(format!(
                                                                "Metric {}",
                                                                route.metric
                                                            )),
                                                    ),
                                            )
                                    })),
                            ),
                    )
                }),
        )
}
