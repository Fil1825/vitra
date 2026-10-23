//! LRC timestamps: `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx`, `mm:ss:xx` (an older form) and
//! `hh:mm:ss.xx`. Minutes may exceed 59.

/// Milliseconds of a timestamp's inside (without the brackets), or `None` if it isn't one.
pub fn parse_timestamp(text: &str) -> Option<u64> {
    let text = text.trim();
    let parts: Vec<&str> = text.split(':').collect();
    let (hours, minutes, seconds, fraction) = match parts.as_slice() {
        [m, s] => {
            let (s, f) = split_fraction(s);
            ("0", *m, s, f)
        }
        // `hh:mm:ss.xx` has a dot; without one it's the older `mm:ss:xx` (hundredths after a
        // colon), which is far more common in LRC files than hours.
        [h, m, s] if s.contains('.') => {
            let (s, f) = split_fraction(s);
            (*h, *m, s, f)
        }
        [m, s, f] => ("0", *m, *s, *f),
        _ => return None,
    };
    let number = |s: &str| -> Option<u64> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let hours = number(hours)?;
    let minutes = number(minutes)?;
    let seconds = number(seconds)?;
    if seconds >= 60 || (hours > 0 && minutes >= 60) {
        return None;
    }
    let millis = match fraction.len() {
        0 => 0,
        1..=3 => number(fraction)? * 10u64.pow(3 - fraction.len() as u32),
        // Finer than milliseconds: keep the milliseconds.
        _ => number(&fraction[..3])?,
    };
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn split_fraction(seconds: &str) -> (&str, &str) {
    seconds.split_once('.').unwrap_or((seconds, ""))
}

/// `mm:ss.xx`, the most widely read form (hundredths, rounded).
pub fn format_timestamp(millis: u64) -> String {
    let centis = (millis + 5) / 10;
    let minutes = centis / 6000;
    let seconds = centis / 100 % 60;
    let hundredths = centis % 100;
    format!("{minutes:02}:{seconds:02}.{hundredths:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_usual_forms() {
        assert_eq!(parse_timestamp("00:12.34"), Some(12_340));
        assert_eq!(parse_timestamp("01:02.5"), Some(62_500));
        assert_eq!(parse_timestamp("01:02.567"), Some(62_567));
        assert_eq!(parse_timestamp("01:02.5678"), Some(62_567));
        assert_eq!(parse_timestamp("3:07"), Some(187_000));
        assert_eq!(parse_timestamp(" 00:01.00 "), Some(1_000));
        // Minutes past an hour without an hours field.
        assert_eq!(parse_timestamp("75:00.00"), Some(4_500_000));
        // The colon form of the fraction.
        assert_eq!(parse_timestamp("00:12:34"), Some(12_340));
        // Hours with a fraction.
        assert_eq!(parse_timestamp("1:02:03.50"), Some(3_723_500));
    }

    #[test]
    fn rejects_what_is_not_a_timestamp() {
        for text in [
            "", "ar", "00:60.00", "aa:bb", "00:", ":12", "1:2:3:4", "-1:00", "00:1x.0",
        ] {
            assert_eq!(parse_timestamp(text), None, "{text}");
        }
    }

    #[test]
    fn formats_hundredths() {
        assert_eq!(format_timestamp(0), "00:00.00");
        assert_eq!(format_timestamp(12_345), "00:12.35");
        assert_eq!(format_timestamp(62_994), "01:02.99");
        assert_eq!(format_timestamp(62_996), "01:03.00");
        assert_eq!(format_timestamp(4_500_000), "75:00.00");
    }
}
