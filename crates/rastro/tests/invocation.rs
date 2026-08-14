//! Turning a clock reading into the format's integer type.

use std::time::{Duration, UNIX_EPOCH};

use rastro::collectors::seconds_since_epoch;

#[test]
fn seconds_since_epoch_counts_from_1970() {
    // Arrange
    let reading = UNIX_EPOCH + Duration::from_secs(1_786_632_455);

    // Act
    let seconds = seconds_since_epoch(reading).expect("a reading after 1970 is representable");

    // Assert
    assert_eq!(seconds, 1_786_632_455);
}

#[test]
fn seconds_since_epoch_refuses_a_clock_set_before_1970() {
    // Arrange: a box with a dead RTC really does come up in 1969.
    let reading = UNIX_EPOCH - Duration::from_secs(1);

    // Act
    let result = seconds_since_epoch(reading);

    // Assert: recorded as a failed facet, never as a wrapped-around timestamp.
    let failure = result.expect_err("a pre-1970 clock cannot be timestamped");
    assert!(
        failure.to_string().contains("before 1970"),
        "the operator needs to know which clock is wrong, got: {failure}"
    );
}

#[test]
fn seconds_since_epoch_accepts_the_epoch_itself() {
    // Act & Assert
    assert_eq!(seconds_since_epoch(UNIX_EPOCH), Ok(0));
}
