//! The wall clock for mods (kindle.clock, kindle.date): seconds since 1970 and the calendar date
//! of a moment, in the player's local time or UTC.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970-01-01 00:00 UTC.
pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Days since 1970-01-01 of a calendar date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The calendar date of a day since 1970-01-01: (year, month, day).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// How far the player's local time is ahead of UTC now, in seconds (Windows' own clocks).
pub fn local_offset() -> i64 {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let secs = |t: &windows::Win32::Foundation::SYSTEMTIME| {
        days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 86400 + t.wHour as i64 * 3600 + t.wMinute as i64 * 60
    };
    secs(&local) - secs(&utc)
}

#[derive(Debug, PartialEq)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    /// 1 Monday .. 7 Sunday.
    pub weekday: i64,
    /// 1 .. 366.
    pub yearday: i64,
}

/// The date at `seconds` since 1970, shifted by `offset` seconds (local time: local_offset()).
pub fn date(seconds: f64, offset: i64) -> Date {
    let t = seconds.floor() as i64 + offset;
    let days = t.div_euclid(86400);
    let rest = t.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    Date {
        year,
        month,
        day,
        hour: rest / 3600,
        minute: rest % 3600 / 60,
        second: rest % 60,
        // 1970-01-01 was a Thursday.
        weekday: (days + 3).rem_euclid(7) + 1,
        yearday: days - days_from_civil(year, 1, 1) + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date(0.0, 0), Date { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, weekday: 4, yearday: 1 });
        // 2026-11-05 14:30:15 UTC, a Thursday, the 309th day.
        let t = (days_from_civil(2026, 11, 5) * 86400 + 14 * 3600 + 30 * 60 + 15) as f64;
        assert_eq!(date(t, 0), Date { year: 2026, month: 11, day: 5, hour: 14, minute: 30, second: 15, weekday: 4, yearday: 309 });
        // Ten hours ahead crosses midnight.
        assert_eq!(date(t, 10 * 3600).day, 6);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }
}
