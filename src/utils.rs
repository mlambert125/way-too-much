pub fn get_wayland_string_bytes(s: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(s.len() as u32 + 1).to_le_bytes());
    bytes.extend_from_slice(s.as_bytes());
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    bytes
}
