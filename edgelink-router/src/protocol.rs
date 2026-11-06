// Keep this wire format in sync with edgelink-beacon/src/protocol.rs.
pub const DISCOVERY_PORT: u16 = 40_191;
pub const QUERY_PREFIX: &str = "EDGE-LINUX-DISCOVER/1 ";
pub const RESPONSE_PREFIX: &str = "EDGE-LINUX-BOARD/1 ";
pub const NONCE_HEX_LEN: usize = 32;

pub fn make_query(nonce: &str) -> Option<String> {
    (nonce.len() == NONCE_HEX_LEN && nonce.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("{QUERY_PREFIX}{nonce}"))
}

pub fn response_matches(packet: &[u8], nonce: &str) -> bool {
    std::str::from_utf8(packet)
        .ok()
        .and_then(|text| text.strip_prefix(RESPONSE_PREFIX))
        == Some(nonce)
}
