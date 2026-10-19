use kami_core::*;

#[test]
fn queue_pressure_rejects_new_admission_without_dropping_existing_work() {
    let limits = QueueLimits::new(2, 10);
    let empty = QueueUsage::new(0, 0);
    let one = empty.admit(limits, 6).unwrap();
    assert_eq!((one.messages(), one.bytes()), (1, 6));
    assert_eq!(one.admit(limits, 5), Err(QueuePressure::Bytes));
    let full = one.admit(limits, 4).unwrap();
    assert_eq!((full.messages(), full.bytes()), (2, 10));
    assert_eq!(full.admit(limits, 0), Err(QueuePressure::Messages));
    assert_eq!(empty, QueueUsage::new(0, 0));
    assert_eq!(one, QueueUsage::new(1, 6));
    assert_eq!(
        empty.admit(QueueLimits::new(0, 10), 0),
        Err(QueuePressure::Messages)
    );
    assert_eq!(
        empty.admit(QueueLimits::new(1, 0), 1),
        Err(QueuePressure::Bytes)
    );
    let unlimited = QueueLimits::new(u64::MAX, u64::MAX);
    assert_eq!(
        QueueUsage::new(u64::MAX, 0).admit(unlimited, 0),
        Err(QueuePressure::Messages)
    );
    assert_eq!(
        QueueUsage::new(0, u64::MAX).admit(unlimited, 1),
        Err(QueuePressure::Bytes)
    );
    assert_eq!(
        QueueUsage::new(1, 11).admit(limits, 0),
        Err(QueuePressure::Bytes)
    );
}
