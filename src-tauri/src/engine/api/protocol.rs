pub const CURRENT_PROTOCOL_VERSION: u32 = 1;
pub const MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION: u32 = 1;
pub const MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION: u32 = CURRENT_PROTOCOL_VERSION;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_client_range_is_explicit_and_bounded() {
        assert!(
            (MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION..=MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION)
                .contains(&CURRENT_PROTOCOL_VERSION)
        );
        assert!(MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION <= MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION);
    }
}
