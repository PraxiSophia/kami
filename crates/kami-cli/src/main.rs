#![forbid(unsafe_code)]

fn main() {
    let _ = (
        kami_core::MessageId::from_u128(0),
        kami_protocol::M1_PROTOCOL_VERSION,
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_scaffold_exposes_the_protocol_boundary() {
        assert_eq!(kami_protocol::M1_PROTOCOL_VERSION, 1);
    }
}
