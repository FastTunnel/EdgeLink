//! Receive discovery requests on the wildcard socket, but only answer requests
//! that arrived on a physical, non-wireless Ethernet interface.

use std::{
    ffi::{CStr, OsStr},
    io,
    mem::{size_of, zeroed},
    net::{Ipv4Addr, SocketAddrV4},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
};

use crate::protocol;

const PACKET_CAPACITY: usize = 256;

pub fn run(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
    let enabled: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            std::os::fd::AsRawFd::as_raw_fd(&socket),
            libc::IPPROTO_IP,
            libc::IP_PKTINFO,
            (&enabled as *const libc::c_int).cast(),
            size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error().into());
    }

    println!("board discovery responder listening on UDP *:{port} (wired Ethernet only)");
    let mut packet = [0_u8; PACKET_CAPACITY];

    loop {
        let Some((length, peer, ingress, local_address)) =
            (match receive_packet(socket.as_raw_fd(), &mut packet) {
                Ok(value) => value,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            })
        else {
            continue;
        };

        if !is_physical_ethernet(ingress) {
            continue;
        }

        let Some(nonce) = protocol::parse_query(&packet[..length]) else {
            continue;
        };
        let Some(response) = protocol::make_response(nonce) else {
            continue;
        };

        if let Err(error) = send_on_interface(
            socket.as_raw_fd(),
            response.as_bytes(),
            peer,
            ingress,
            local_address,
        ) {
            eprintln!("could not reply to {peer} on interface index {ingress}: {error}");
        }
    }
}

fn receive_packet(
    fd: libc::c_int,
    packet: &mut [u8],
) -> io::Result<Option<(usize, SocketAddrV4, u32, Ipv4Addr)>> {
    let mut peer: libc::sockaddr_in = unsafe { zeroed() };
    let mut iov = libc::iovec {
        iov_base: packet.as_mut_ptr().cast(),
        iov_len: packet.len(),
    };
    // Keep ancillary storage aligned for cmsghdr/in_pktinfo.
    let mut control = [0_usize; 16];
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_name = (&mut peer as *mut libc::sockaddr_in).cast();
    message.msg_namelen = size_of::<libc::sockaddr_in>() as libc::socklen_t;
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = size_of_val(&control);

    let received = unsafe { libc::recvmsg(fd, &mut message, 0) };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }
    if message.msg_flags & libc::MSG_TRUNC != 0 || peer.sin_family as libc::c_int != libc::AF_INET {
        return Ok(None);
    }

    let mut packet_info = None;
    let mut header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    while !header.is_null() {
        let item = unsafe { &*header };
        if item.cmsg_level == libc::IPPROTO_IP && item.cmsg_type == libc::IP_PKTINFO {
            let info =
                unsafe { ptr::read_unaligned(libc::CMSG_DATA(header).cast::<libc::in_pktinfo>()) };
            packet_info = u32::try_from(info.ipi_ifindex).ok().map(|index| {
                (
                    index,
                    Ipv4Addr::from(info.ipi_spec_dst.s_addr.to_ne_bytes()),
                )
            });
            break;
        }
        header = unsafe { libc::CMSG_NXTHDR(&message, header) };
    }

    let Some((interface_index, local_address)) = packet_info else {
        return Ok(None);
    };
    let address = Ipv4Addr::from(peer.sin_addr.s_addr.to_ne_bytes());
    let port = u16::from_be(peer.sin_port);
    Ok(Some((
        received as usize,
        SocketAddrV4::new(address, port),
        interface_index,
        local_address,
    )))
}

fn is_physical_ethernet(index: u32) -> bool {
    let mut name = [0 as libc::c_char; libc::IF_NAMESIZE];
    if unsafe { libc::if_indextoname(index, name.as_mut_ptr()) }.is_null() {
        return false;
    }
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    let name = OsStr::from_bytes(name.to_bytes());
    let interface = Path::new("/sys/class/net").join(name);

    // ARPHRD_ETHER is 1. Physical device presence excludes bridges, veths,
    // virtual switches and VLANs. Wi-Fi exposes the separate "wireless" dir.
    let ethernet_type = std::fs::read_to_string(interface.join("type"))
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        == Some(1);
    ethernet_type && interface.join("device").exists() && !interface.join("wireless").exists()
}

fn send_on_interface(
    fd: libc::c_int,
    payload: &[u8],
    peer: SocketAddrV4,
    interface_index: u32,
    local_address: Ipv4Addr,
) -> io::Result<()> {
    let mut destination: libc::sockaddr_in = unsafe { zeroed() };
    destination.sin_family = libc::AF_INET as libc::sa_family_t;
    destination.sin_port = peer.port().to_be();
    destination.sin_addr = libc::in_addr {
        s_addr: u32::from_ne_bytes(peer.ip().octets()),
    };

    let mut iov = libc::iovec {
        iov_base: payload.as_ptr().cast_mut().cast(),
        iov_len: payload.len(),
    };
    let mut control = [0_usize; 16];
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_name = (&mut destination as *mut libc::sockaddr_in).cast();
    message.msg_namelen = size_of::<libc::sockaddr_in>() as libc::socklen_t;
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen =
        unsafe { libc::CMSG_SPACE(size_of::<libc::in_pktinfo>() as u32) as usize };

    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    if header.is_null() {
        return Err(io::Error::other("could not allocate IPv4 packet metadata"));
    }
    unsafe {
        (*header).cmsg_level = libc::IPPROTO_IP;
        (*header).cmsg_type = libc::IP_PKTINFO;
        (*header).cmsg_len = libc::CMSG_LEN(size_of::<libc::in_pktinfo>() as u32) as usize;
        let info = libc::in_pktinfo {
            ipi_ifindex: interface_index as libc::c_int,
            ipi_spec_dst: libc::in_addr {
                s_addr: u32::from_ne_bytes(local_address.octets()),
            },
            ipi_addr: libc::in_addr { s_addr: 0 },
        };
        ptr::write(libc::CMSG_DATA(header).cast::<libc::in_pktinfo>(), info);
    }

    let sent = unsafe { libc::sendmsg(fd, &message, 0) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    if sent as usize != payload.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "partial UDP response",
        ));
    }
    Ok(())
}

use std::mem::size_of_val;

use std::os::fd::AsRawFd;
