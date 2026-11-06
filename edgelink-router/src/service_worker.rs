use crate::{scanner, settings::Settings, windows_net};
use std::{
    net::Ipv4Addr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn run(stop: Arc<AtomicBool>) {
    let mut last_interface_index = None;
    let mut last_interface_name = None;
    let mut consecutive_misses = 0_u8;
    let mut fallback_applied = false;
    while !stop.load(Ordering::Acquire) {
        let settings = Settings::load();
        let outcome = scan_and_update(&settings);
        match outcome {
            Ok((interface_index, Some(_board))) => {
                last_interface_index = Some(interface_index);
                last_interface_name = Some(settings.interface.clone());
                consecutive_misses = 0;
                fallback_applied = false;
            }
            Ok((interface_index, None)) => {
                last_interface_index = Some(interface_index);
                last_interface_name = Some(settings.interface.clone());
                consecutive_misses = consecutive_misses.saturating_add(1);
            }
            Err(_error) => {
                consecutive_misses = consecutive_misses.saturating_add(1);
            }
        }

        if consecutive_misses >= 3 && !fallback_applied {
            if let Some(interface_index) = last_interface_index {
                let routes_removed = windows_net::remove_routes(interface_index).is_ok();
                let interface_name = last_interface_name
                    .as_deref()
                    .unwrap_or(&settings.interface);
                let dhcp_restored = windows_net::restore_interface_to_dhcp(interface_name).is_ok();
                fallback_applied = routes_removed && dhcp_restored;
            } else {
                fallback_applied = true;
            }
        }

        let delay = Duration::from_secs(settings.interval_seconds.clamp(1, 300));
        let mut elapsed = Duration::ZERO;
        while elapsed < delay && !stop.load(Ordering::Acquire) {
            let slice = Duration::from_millis(200).min(delay - elapsed);
            thread::sleep(slice);
            elapsed += slice;
        }
    }
    if let Some(interface_index) = last_interface_index {
        let _ = windows_net::remove_routes(interface_index);
        let fallback_settings = Settings::load();
        let interface_name = last_interface_name
            .as_deref()
            .unwrap_or(&fallback_settings.interface);
        let _ = windows_net::restore_interface_to_dhcp(interface_name);
    }
}

fn scan_and_update(settings: &Settings) -> Result<(u32, Option<Ipv4Addr>), String> {
    let (interface_index, addresses) = windows_net::interface_addresses(&settings.interface)?;
    if addresses.is_empty() {
        return Ok((interface_index, None));
    }
    let Some(board) = scanner::discover(&addresses, settings.discovery_port)? else {
        return Ok((interface_index, None));
    };
    windows_net::update_routes(interface_index, board)?;
    Ok((interface_index, Some(board)))
}
