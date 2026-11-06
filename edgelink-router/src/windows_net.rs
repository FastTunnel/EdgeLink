use serde::Deserialize;
use std::{mem, net::Ipv4Addr, ptr};

use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR},
    NetworkManagement::IpHelper::{
        CreateIpForwardEntry2, DeleteIpForwardEntry2, FreeMibTable, GetAdaptersAddresses,
        GetIfTable2, GetIpForwardTable2, InitializeIpForwardEntry, GAA_FLAG_INCLUDE_GATEWAYS,
        GAA_FLAG_INCLUDE_PREFIX, IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_DHCP_ENABLED, MIB_IF_TABLE2,
        MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2,
    },
    Networking::WinSock::{
        IpDadStatePreferred, AF_INET, IN_ADDR, IN_ADDR_0, MIB_IPPROTO_OTHER, SOCKADDR_IN,
        SOCKADDR_INET, SOCKET_ADDRESS,
    },
};

const ROUTE_PREFIXES: [(Ipv4Addr, u8); 2] = [
    (Ipv4Addr::new(0, 0, 0, 0), 1),
    (Ipv4Addr::new(128, 0, 0, 0), 1),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterIpv4Entry {
    pub address: Ipv4Addr,
    pub prefix_length: u8,
    pub netmask: Ipv4Addr,
    pub broadcast: Option<Ipv4Addr>,
    pub preferred: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRouteEntry {
    pub destination: String,
    pub next_hop: Ipv4Addr,
    pub metric: u32,
    pub is_managed_board_route: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterDetails {
    pub friendly_name: String,
    pub description: String,
    pub adapter_guid: String,
    pub if_index: u32,
    pub if_type_label: String,
    pub oper_status_label: String,
    pub is_up: bool,
    pub mac_address: String,
    pub mtu: u32,
    pub dhcp_enabled: bool,
    pub ipv4_unbound_or_bridged: bool,
    pub dhcp_server: Option<Ipv4Addr>,
    pub tx_speed_label: String,
    pub rx_speed_label: String,
    pub ipv4_metric: u32,
    pub dns_suffix: String,
    pub ipv4_addresses: Vec<AdapterIpv4Entry>,
    pub gateways: Vec<Ipv4Addr>,
    pub dns_servers: Vec<Ipv4Addr>,
    pub managed_board_ip: Option<Ipv4Addr>,
    pub active_routes: Vec<AdapterRouteEntry>,
}

pub fn inspect_all_adapters() -> Result<Vec<AdapterDetails>, String> {
    let mut storage = adapter_buffer()?;
    let all_routes = route_rows().ok();

    let mut list = Vec::new();
    let mut adapter = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        let current = unsafe { &*adapter };
        if current.FriendlyName.is_null() {
            adapter = current.Next;
            continue;
        }
        let friendly_name = unsafe { read_wide_string(current.FriendlyName) };
        if friendly_name.is_empty() {
            adapter = current.Next;
            continue;
        }

        let description = if current.Description.is_null() {
            "未知适配器".to_owned()
        } else {
            unsafe { read_wide_string(current.Description) }
        };
        let adapter_guid = if current.AdapterName.is_null() {
            "-".to_owned()
        } else {
            unsafe { read_ansi_string(current.AdapterName) }
        };
        let dns_suffix = if current.DnsSuffix.is_null() {
            String::new()
        } else {
            unsafe { read_wide_string(current.DnsSuffix) }
        };

        let if_index = unsafe { current.Anonymous1.Anonymous.IfIndex };
        let flags = unsafe { current.Anonymous2.Flags };
        let dhcp_enabled = (flags & IP_ADAPTER_DHCP_ENABLED) != 0;
        let dhcp_server = unsafe { ipv4_from_socket_address(current.Dhcpv4Server) }
            .filter(|ip| !ip.is_unspecified());

        let mac_len = (current.PhysicalAddressLength as usize).min(current.PhysicalAddress.len());
        let mac_address = if mac_len == 0 {
            "无物理地址".to_owned()
        } else {
            current.PhysicalAddress[..mac_len]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join("-")
        };

        let is_up = current.OperStatus == 1;
        let oper_status_label = match current.OperStatus {
            1 => "已连接 (Up)".to_owned(),
            2 => "已断开 (Down)".to_owned(),
            3 => "测试中 (Testing)".to_owned(),
            5 => "休眠 (Dormant)".to_owned(),
            6 => "缺少组件 (NotPresent)".to_owned(),
            7 => "下层断开 (LowerLayerDown)".to_owned(),
            _ => "未知状态".to_owned(),
        };
        let if_type_label = match current.IfType {
            6 => "以太网 (Ethernet)".to_owned(),
            71 => "无线网卡 (IEEE 802.11)".to_owned(),
            24 => "软件回环 (Loopback)".to_owned(),
            53 => "虚拟专有接口 (Virtual)".to_owned(),
            131 => "隧道接口 (Tunnel)".to_owned(),
            other => format!("接口类型 #{other}"),
        };

        let tx_speed_label = format_link_speed(current.TransmitLinkSpeed);
        let rx_speed_label = format_link_speed(current.ReceiveLinkSpeed);

        let mut ipv4_addresses = Vec::new();
        let mut unicast = current.FirstUnicastAddress;
        while !unicast.is_null() {
            let entry = unsafe { &*unicast };
            if let Some(address) = unsafe { ipv4_from_socket_address(entry.Address) } {
                let prefix_length = entry.OnLinkPrefixLength.min(32);
                let mask_u32 = if prefix_length == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix_length)
                };
                let netmask = Ipv4Addr::from(mask_u32);
                let broadcast = if (1..=30).contains(&prefix_length) {
                    Some(Ipv4Addr::from(u32::from(address) | !mask_u32))
                } else {
                    None
                };
                let preferred = entry.DadState == IpDadStatePreferred;
                ipv4_addresses.push(AdapterIpv4Entry {
                    address,
                    prefix_length,
                    netmask,
                    broadcast,
                    preferred,
                });
            }
            unicast = entry.Next;
        }

        let mut gateways = Vec::new();
        let mut gw = current.FirstGatewayAddress;
        while !gw.is_null() {
            let entry = unsafe { &*gw };
            if let Some(ip) = unsafe { ipv4_from_socket_address(entry.Address) } {
                if !gateways.contains(&ip) {
                    gateways.push(ip);
                }
            }
            gw = entry.Next;
        }

        let mut dns_servers = Vec::new();
        let mut dns = current.FirstDnsServerAddress;
        while !dns.is_null() {
            let entry = unsafe { &*dns };
            if let Some(ip) = unsafe { ipv4_from_socket_address(entry.Address) } {
                if !dns_servers.contains(&ip) {
                    dns_servers.push(ip);
                }
            }
            dns = entry.Next;
        }

        let mut managed_board_ip = None;
        let mut active_routes = Vec::new();
        if let Some(rows) = &all_routes {
            for row in rows.iter() {
                if row.InterfaceIndex != if_index {
                    continue;
                }
                let prefix = unsafe { ipv4_from_sockaddr(row.DestinationPrefix.Prefix) };
                let prefix_len = row.DestinationPrefix.PrefixLength;
                let next_hop = unsafe { ipv4_from_sockaddr(row.NextHop) };
                let is_managed = managed_prefix_index(row, if_index).is_some()
                    && row.Protocol == MIB_IPPROTO_OTHER
                    && row.Metric == 1;
                if is_managed && !next_hop.is_unspecified() {
                    managed_board_ip = Some(next_hop);
                }
                if prefix_len == 0 && !next_hop.is_unspecified() && !gateways.contains(&next_hop) {
                    gateways.push(next_hop);
                }
                active_routes.push(AdapterRouteEntry {
                    destination: format!("{prefix}/{prefix_len}"),
                    next_hop,
                    metric: row.Metric,
                    is_managed_board_route: is_managed,
                });
            }
        }
        active_routes.sort_by_key(|r| (!r.is_managed_board_route, r.metric, r.destination.clone()));

        list.push(AdapterDetails {
            friendly_name,
            description,
            adapter_guid,
            if_index,
            if_type_label,
            oper_status_label,
            is_up,
            mac_address,
            mtu: current.Mtu,
            dhcp_enabled,
            ipv4_unbound_or_bridged: false,
            dhcp_server,
            tx_speed_label,
            rx_speed_label,
            ipv4_metric: current.Ipv4Metric,
            dns_suffix,
            ipv4_addresses,
            gateways,
            dns_servers,
            managed_board_ip,
            active_routes,
        });

        adapter = current.Next;
    }

    append_ndis_only_adapters(&mut list, all_routes.as_deref());

    list.sort_by_key(|item| {
        let is_wifi = item.if_type_label.contains("802.11");
        (
            !item.is_up,
            !(is_wifi || !item.ipv4_addresses.is_empty()),
            item.friendly_name.to_lowercase(),
        )
    });
    Ok(list)
}

fn append_ndis_only_adapters(
    list: &mut Vec<AdapterDetails>,
    all_routes: Option<&[MIB_IPFORWARD_ROW2]>,
) {
    let mut table_ptr: *mut MIB_IF_TABLE2 = ptr::null_mut();
    let status = unsafe { GetIfTable2(&mut table_ptr) };
    if status != NO_ERROR || table_ptr.is_null() {
        return;
    }
    struct IfTableGuard(*mut MIB_IF_TABLE2);
    impl Drop for IfTableGuard {
        fn drop(&mut self) {
            unsafe { FreeMibTable(self.0.cast()) };
        }
    }
    let _guard = IfTableGuard(table_ptr);
    let table = unsafe { &*table_ptr };
    let rows =
        unsafe { std::slice::from_raw_parts(table.Table.as_ptr(), table.NumEntries as usize) };

    for row in rows {
        // Skip NDIS lightweight filter layers (bit 1 = FilterInterface)
        if (row.InterfaceAndOperStatusFlags._bitfield & 0x02) != 0 {
            continue;
        }
        let is_hardware = (row.InterfaceAndOperStatusFlags._bitfield & 0x01) != 0;
        let is_up = row.OperStatus == 1;
        if row.OperStatus == 6 && !is_hardware {
            continue;
        }
        let friendly_name = unsafe { read_wide_string(row.Alias.as_ptr()) };
        if friendly_name.is_empty() {
            continue;
        }
        if list.iter().any(|existing| {
            existing.if_index == row.InterfaceIndex
                || existing.friendly_name.eq_ignore_ascii_case(&friendly_name)
        }) {
            continue;
        }

        let description = unsafe { read_wide_string(row.Description.as_ptr()) };
        if description.starts_with("WAN Miniport")
            || description.contains("Kernel Debug")
            || description.contains("Teredo")
            || description.contains("IP-HTTPS")
            || description.contains("6to4")
        {
            continue;
        }

        let mac_len = (row.PhysicalAddressLength as usize).min(row.PhysicalAddress.len());
        let mac_address = if mac_len == 0 {
            "无物理地址".to_owned()
        } else {
            row.PhysicalAddress[..mac_len]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join("-")
        };

        let oper_status_label = match row.OperStatus {
            1 => "已连接 (Up)".to_owned(),
            2 => "已断开 (Down)".to_owned(),
            3 => "测试中 (Testing)".to_owned(),
            5 => "休眠 (Dormant)".to_owned(),
            6 => "未就绪 (NotPresent)".to_owned(),
            7 => "下层断开 (LowerLayerDown)".to_owned(),
            _ => "未知状态".to_owned(),
        };
        let if_type_label = match row.Type {
            6 => "以太网 (Ethernet)".to_owned(),
            71 => "无线网卡 (IEEE 802.11)".to_owned(),
            24 => "软件回环 (Loopback)".to_owned(),
            53 => "虚拟交换/网桥 (Virtual)".to_owned(),
            131 => "隧道接口 (Tunnel)".to_owned(),
            other => format!("接口类型 #{other}"),
        };

        let g = row.InterfaceGuid;
        let adapter_guid = format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            g.data1,
            g.data2,
            g.data3,
            g.data4[0],
            g.data4[1],
            g.data4[2],
            g.data4[3],
            g.data4[4],
            g.data4[5],
            g.data4[6],
            g.data4[7]
        );

        let mut active_routes = Vec::new();
        if let Some(route_rows) = all_routes {
            for r in route_rows {
                if r.InterfaceIndex == row.InterfaceIndex {
                    let prefix = unsafe { ipv4_from_sockaddr(r.DestinationPrefix.Prefix) };
                    let prefix_len = r.DestinationPrefix.PrefixLength;
                    let next_hop = unsafe { ipv4_from_sockaddr(r.NextHop) };
                    active_routes.push(AdapterRouteEntry {
                        destination: format!("{prefix}/{prefix_len}"),
                        next_hop,
                        metric: r.Metric,
                        is_managed_board_route: false,
                    });
                }
            }
        }

        list.push(AdapterDetails {
            friendly_name,
            description,
            adapter_guid,
            if_index: row.InterfaceIndex,
            if_type_label,
            oper_status_label,
            is_up,
            mac_address,
            mtu: row.Mtu,
            dhcp_enabled: false,
            ipv4_unbound_or_bridged: true,
            dhcp_server: None,
            tx_speed_label: format_link_speed(row.TransmitLinkSpeed),
            rx_speed_label: format_link_speed(row.ReceiveLinkSpeed),
            ipv4_metric: 0,
            dns_suffix: String::new(),
            ipv4_addresses: Vec::new(),
            gateways: Vec::new(),
            dns_servers: Vec::new(),
            managed_board_ip: None,
            active_routes,
        });
    }
}

fn format_link_speed(bps: u64) -> String {
    if bps == 0 || bps == u64::MAX {
        return "自动 / 未知".to_owned();
    }
    if bps >= 1_000_000_000 {
        let gbps = bps as f64 / 1_000_000_000.0;
        if (gbps - gbps.round()).abs() < 0.05 {
            format!("{:.0} Gbps", gbps)
        } else {
            format!("{:.1} Gbps", gbps)
        }
    } else if bps >= 1_000_000 {
        format!("{} Mbps", bps / 1_000_000)
    } else if bps >= 1_000 {
        format!("{} Kbps", bps / 1_000)
    } else {
        format!("{bps} bps")
    }
}

pub fn interface_addresses(name: &str) -> Result<(u32, Vec<(Ipv4Addr, u8)>), String> {
    let mut storage = adapter_buffer()?;
    let mut adapter = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        let current = unsafe { &*adapter };
        if current.FriendlyName.is_null() {
            adapter = current.Next;
            continue;
        }
        let friendly_name = unsafe { read_wide_string(current.FriendlyName) };
        if friendly_name.eq_ignore_ascii_case(name) {
            let interface_index = unsafe { current.Anonymous1.Anonymous.IfIndex };
            let mut addresses = Vec::new();
            let mut unicast = current.FirstUnicastAddress;
            while !unicast.is_null() {
                let entry = unsafe { &*unicast };
                if !entry.Address.lpSockaddr.is_null()
                    && entry.OnLinkPrefixLength <= 30
                    && entry.DadState == IpDadStatePreferred
                    && unsafe { (*entry.Address.lpSockaddr).sa_family as u16 == AF_INET }
                {
                    let sockaddr = unsafe { &*entry.Address.lpSockaddr.cast::<SOCKADDR_IN>() };
                    let address = unsafe { ipv4_from_in_addr(sockaddr.sin_addr) };
                    addresses.push((address, entry.OnLinkPrefixLength));
                }
                unicast = entry.Next;
            }
            return Ok((interface_index, addresses));
        }
        adapter = current.Next;
    }

    Err(format!("network interface not found: {name}"))
}

pub fn interface_names() -> Result<Vec<String>, String> {
    let mut storage = adapter_buffer()?;
    let mut names = Vec::new();
    let mut adapter = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        let current = unsafe { &*adapter };
        if !current.FriendlyName.is_null() {
            let name = unsafe { read_wide_string(current.FriendlyName) };
            if !name.is_empty() {
                names.push(name);
            }
        }
        adapter = current.Next;
    }
    names.sort_unstable_by_key(|name| name.to_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(names)
}

fn adapter_buffer() -> Result<Vec<u64>, String> {
    // Microsoft's recommended working buffer is 15 KiB. Use u64 elements to
    // keep the buffer suitably aligned for IP_ADAPTER_ADDRESSES structures.
    let flags = GAA_FLAG_INCLUDE_PREFIX | GAA_FLAG_INCLUDE_GATEWAYS;
    let mut storage = vec![0_u64; (15_usize * 1024).div_ceil(mem::size_of::<u64>())];
    let mut size_bytes = (storage.len() * mem::size_of::<u64>()) as u32;
    let status = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            ptr::null(),
            storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
            &mut size_bytes,
        )
    };
    if status == ERROR_BUFFER_OVERFLOW {
        storage.resize((size_bytes as usize).div_ceil(mem::size_of::<u64>()), 0);
        let retry = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                flags,
                ptr::null(),
                storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut size_bytes,
            )
        };
        if retry != NO_ERROR {
            return Err(format!(
                "GetAdaptersAddresses failed with Windows error {retry}"
            ));
        }
    } else if status != NO_ERROR {
        return Err(format!(
            "GetAdaptersAddresses failed with Windows error {status}"
        ));
    }
    Ok(storage)
}

unsafe fn read_wide_string(pointer: *const u16) -> String {
    let mut length = 0;
    while *pointer.add(length) != 0 {
        length += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
}

unsafe fn read_ansi_string(pointer: *const u8) -> String {
    let mut length = 0;
    while *pointer.add(length) != 0 {
        length += 1;
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(pointer, length)).into_owned()
}

unsafe fn ipv4_from_socket_address(socket_address: SOCKET_ADDRESS) -> Option<Ipv4Addr> {
    if socket_address.lpSockaddr.is_null() {
        return None;
    }
    if (*socket_address.lpSockaddr).sa_family as u16 != AF_INET {
        return None;
    }
    let sockaddr = &*socket_address.lpSockaddr.cast::<SOCKADDR_IN>();
    Some(ipv4_from_in_addr(sockaddr.sin_addr))
}

pub fn update_routes(interface_index: u32, board: Ipv4Addr) -> Result<(), String> {
    let rows = route_rows()?;
    let mut stale = Vec::new();
    let mut present = [false; 2];

    for row in rows.iter() {
        let Some(prefix_index) = managed_prefix_index(row, interface_index) else {
            continue;
        };
        let next_hop = unsafe { ipv4_from_sockaddr(row.NextHop) };
        if row.Protocol == MIB_IPPROTO_OTHER && row.Metric == 1 && next_hop != board {
            stale.push(*row);
        }
        if next_hop == board && row.Metric == 1 {
            present[prefix_index] = true;
        }
    }
    drop(rows);

    let mut removed = Vec::new();
    for row in stale {
        check_route_status("DeleteIpForwardEntry2", unsafe {
            DeleteIpForwardEntry2(&row)
        })?;
        removed.push(row);
    }

    let mut added = Vec::new();
    for (index, (network, prefix_length)) in ROUTE_PREFIXES.iter().enumerate() {
        if present[index] {
            continue;
        }
        let row = make_route(interface_index, *network, *prefix_length, board);
        let status = unsafe { CreateIpForwardEntry2(&row) };
        if status != NO_ERROR {
            for created in added.iter().rev() {
                unsafe { DeleteIpForwardEntry2(created) };
            }
            for old in &removed {
                unsafe { CreateIpForwardEntry2(old) };
            }
            return Err(format!(
                "CreateIpForwardEntry2 failed with Windows error {status}"
            ));
        }
        added.push(row);
    }

    Ok(())
}

pub fn remove_routes(interface_index: u32) -> Result<(), String> {
    let rows = route_rows()?;
    for row in rows.iter() {
        if managed_prefix_index(row, interface_index).is_some()
            && row.Protocol == MIB_IPPROTO_OTHER
            && row.Metric == 1
        {
            check_route_status("DeleteIpForwardEntry2", unsafe {
                DeleteIpForwardEntry2(row)
            })?;
        }
    }
    Ok(())
}

/// Restores one adapter's IPv4 addressing and DNS server selection to DHCP.
/// The Windows service runs as LocalSystem, so WMI's native COM interface can
/// apply these settings without invoking PowerShell or a shell command.
pub fn restore_interface_to_dhcp(interface_name: &str) -> Result<(), String> {
    let (interface_index, _) = interface_addresses(interface_name)?;
    let connection = wmi::WMIConnection::new()
        .map_err(|error| format!("cannot connect to Windows WMI: {error}"))?;
    let adapters: Vec<WmiNetworkAdapterConfiguration> = connection
        .raw_query(
            "SELECT __PATH, InterfaceIndex, IPEnabled FROM Win32_NetworkAdapterConfiguration",
        )
        .map_err(|error| format!("cannot query Windows network adapters: {error}"))?;
    let adapter = adapters
        .into_iter()
        .find(|adapter| adapter.interface_index == interface_index && adapter.ip_enabled)
        .ok_or_else(|| format!("WMI could not find IP-enabled adapter {interface_name}"))?;

    let address_result: WmiMethodResult = connection
        .exec_instance_method::<WmiNetworkAdapterConfiguration, WmiMethodResult>(
            &adapter.path,
            "EnableDHCP",
            (),
        )
        .map_err(|error| format!("EnableDHCP failed on {interface_name}: {error}"))?;
    ensure_wmi_success("EnableDHCP", address_result.return_value)?;

    let dns_result: WmiMethodResult = connection
        .exec_instance_method::<WmiNetworkAdapterConfiguration, WmiMethodResult>(
            &adapter.path,
            "SetDNSServerSearchOrder",
            (),
        )
        .map_err(|error| format!("resetting DNS to DHCP failed on {interface_name}: {error}"))?;
    ensure_wmi_success("SetDNSServerSearchOrder", dns_result.return_value)
}

#[derive(Deserialize)]
struct WmiNetworkAdapterConfiguration {
    #[serde(rename = "__PATH")]
    path: String,
    #[serde(rename = "InterfaceIndex")]
    interface_index: u32,
    #[serde(rename = "IPEnabled")]
    ip_enabled: bool,
}

#[derive(Deserialize)]
struct WmiMethodResult {
    #[serde(rename = "ReturnValue")]
    return_value: u32,
}

fn ensure_wmi_success(method: &str, return_value: u32) -> Result<(), String> {
    match return_value {
        0 => Ok(()),
        1 => Ok(()), // Windows reports success with a reboot required.
        code => Err(format!("{method} returned WMI error {code}")),
    }
}

struct RouteTable(*mut MIB_IPFORWARD_TABLE2);

impl Drop for RouteTable {
    fn drop(&mut self) {
        unsafe { FreeMibTable(self.0.cast()) };
    }
}

struct RouteRows {
    _table: RouteTable,
    rows: Vec<MIB_IPFORWARD_ROW2>,
}

impl std::ops::Deref for RouteRows {
    type Target = [MIB_IPFORWARD_ROW2];

    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

fn route_rows() -> Result<RouteRows, String> {
    let mut table = ptr::null_mut();
    let status = unsafe { GetIpForwardTable2(AF_INET, &mut table) };
    if status != NO_ERROR {
        return Err(format!(
            "GetIpForwardTable2 failed with Windows error {status}"
        ));
    }
    if table.is_null() {
        return Err("GetIpForwardTable2 returned a null table".into());
    }

    let table_guard = RouteTable(table);
    let table_ref = unsafe { &*table };
    let rows = unsafe {
        std::slice::from_raw_parts(table_ref.Table.as_ptr(), table_ref.NumEntries as usize).to_vec()
    };
    Ok(RouteRows {
        _table: table_guard,
        rows,
    })
}

fn managed_prefix_index(row: &MIB_IPFORWARD_ROW2, interface_index: u32) -> Option<usize> {
    if row.InterfaceIndex != interface_index {
        return None;
    }
    let prefix = unsafe { ipv4_from_sockaddr(row.DestinationPrefix.Prefix) };
    ROUTE_PREFIXES.iter().position(|(network, length)| {
        *length == row.DestinationPrefix.PrefixLength && *network == prefix
    })
}

fn make_route(
    interface_index: u32,
    network: Ipv4Addr,
    prefix_length: u8,
    board: Ipv4Addr,
) -> MIB_IPFORWARD_ROW2 {
    let mut row = MIB_IPFORWARD_ROW2::default();
    unsafe { InitializeIpForwardEntry(&mut row) };
    row.InterfaceIndex = interface_index;
    row.DestinationPrefix.Prefix = sockaddr_inet(network);
    row.DestinationPrefix.PrefixLength = prefix_length;
    row.NextHop = sockaddr_inet(board);
    row.Metric = 1;
    row.Protocol = MIB_IPPROTO_OTHER;
    row.ValidLifetime = u32::MAX;
    row.PreferredLifetime = u32::MAX;
    row
}

fn sockaddr_inet(address: Ipv4Addr) -> SOCKADDR_INET {
    let mut sockaddr = SOCKADDR_INET::default();
    sockaddr.Ipv4 = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_port: 0,
        sin_addr: IN_ADDR {
            S_un: IN_ADDR_0 {
                S_addr: u32::from_ne_bytes(address.octets()),
            },
        },
        sin_zero: [0; 8],
    };
    sockaddr
}

unsafe fn ipv4_from_sockaddr(sockaddr: SOCKADDR_INET) -> Ipv4Addr {
    ipv4_from_in_addr(sockaddr.Ipv4.sin_addr)
}

unsafe fn ipv4_from_in_addr(address: IN_ADDR) -> Ipv4Addr {
    Ipv4Addr::from(address.S_un.S_addr.to_ne_bytes())
}

fn check_route_status(operation: &str, status: u32) -> Result<(), String> {
    if status == NO_ERROR {
        Ok(())
    } else {
        Err(format!("{operation} failed with Windows error {status}"))
    }
}
