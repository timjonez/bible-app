/// Dump hex verse ids (`65BE`, `688`) are 1-based indexes into the
/// 31,102-verse Protestant canon (KJV `bt7` order).
pub fn hex_id_to_index(hex: &str) -> Option<u32> {
    let hex = hex.trim();
    if hex.is_empty() {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids() {
        // 1-based: John 1:1 is verse 26046, Ex 6:16 is 1672, Ps 33:6 is 14373.
        assert_eq!(hex_id_to_index("65BE"), Some(0x65BE));
        assert_eq!(0x65BE, 26046);
        assert_eq!(hex_id_to_index("688"), Some(0x688));
        assert_eq!(0x688, 1672);
        assert_eq!(hex_id_to_index("3825"), Some(0x3825));
        assert_eq!(0x3825, 14373);
    }
}
