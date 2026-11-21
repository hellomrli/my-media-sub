//! Date/time formatting shared by APIs and source metadata.

use jiff::{fmt::temporal::DateTimePrinter, tz::Offset, Timestamp};

pub fn unix_now_millis() -> i64 {
    Timestamp::now().as_millisecond()
}

pub fn utc_now_rfc3339() -> String {
    utc_rfc3339(Timestamp::now())
}

/// Preserve the existing RFC 3339 representation: an explicit UTC offset and
/// fractional seconds in groups of three digits, with no zone annotation.
pub fn utc_rfc3339(timestamp: Timestamp) -> String {
    let nanos = timestamp.subsec_nanosecond();
    let precision = if nanos == 0 {
        0
    } else if nanos % 1_000_000 == 0 {
        3
    } else if nanos % 1_000 == 0 {
        6
    } else {
        9
    };
    DateTimePrinter::new()
        .precision(Some(precision))
        .timestamp_with_offset_to_string(&timestamp, Offset::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_timestamps_keep_the_existing_wire_format() {
        for (input, expected) in [
            ("2026-06-29T08:00:00+08:00", "2026-06-29T00:00:00+00:00"),
            ("2026-06-29T00:00:00.1Z", "2026-06-29T00:00:00.100+00:00"),
            (
                "2026-06-29T00:00:00.1234Z",
                "2026-06-29T00:00:00.123400+00:00",
            ),
            (
                "2026-06-29T00:00:00.1234567Z",
                "2026-06-29T00:00:00.123456700+00:00",
            ),
            ("1969-12-31T23:59:59.999Z", "1969-12-31T23:59:59.999+00:00"),
        ] {
            let timestamp = input.parse().unwrap();
            assert_eq!(utc_rfc3339(timestamp), expected);
            assert_eq!(expected.parse::<Timestamp>().unwrap(), timestamp);
        }
    }
}
