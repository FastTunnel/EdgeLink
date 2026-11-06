//! Small UDP responder for discovering the board's current IPv4 address.

mod protocol;
#[cfg(target_os = "linux")]
mod wired_socket;

use std::{env, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("board_discoveryd: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let port = parse_port()?;
    #[cfg(target_os = "linux")]
    return wired_socket::run(port);

    #[cfg(not(target_os = "linux"))]
    Err("edgelink-beacon requires Linux".into())
}

fn parse_port() -> Result<u16, Box<dyn std::error::Error>> {
    let mut port = protocol::DISCOVERY_PORT;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args.next().ok_or("--port requires a UDP port")?.parse()?;
                if port == 0 {
                    return Err("--port cannot be zero".into());
                }
            }
            "--help" | "-h" => {
                println!("Usage: board_discoveryd [--port UDP_PORT]");
                process::exit(0);
            }
            unknown => return Err(format!("unknown argument: {unknown}").into()),
        }
    }
    Ok(port)
}
