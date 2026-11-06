use kami_sender::retry::backoff_ms;
#[test]
fn exponential_policy_has_one_second_base_and_five_minute_cap() {
    assert_eq!(backoff_ms(1), Some(1_000));
    assert_eq!(backoff_ms(8), Some(128_000));
    assert_eq!(backoff_ms(9), Some(256_000));
    assert_eq!(backoff_ms(10), Some(300_000));
    assert_eq!(backoff_ms(u32::MAX), Some(300_000));
    assert_eq!(backoff_ms(0), None);
}
