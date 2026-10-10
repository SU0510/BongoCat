//! Clock formatting and alarm scheduling on civil time. All arithmetic goes
//! through `jiff` so DST gaps/folds and timezone changes behave correctly;
//! nothing here touches the network.

use crate::store::{Alarm, AlarmRepeat};
use jiff::Zoned;

/// `HH:MM` parsed from the alarm's stored civil time.
pub fn parse_hhmm(time: &str) -> Option<(i8, i8)> {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    if parts[0].len() != 2 || parts[1].len() != 2 {
        return None;
    }
    let hour: i8 = parts[0].parse().ok()?;
    let minute: i8 = parts[1].parse().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some((hour, minute))
}

/// The civil date key (`YYYY-MM-DD`) of `now` in the local zone.
pub fn date_key(now: &Zoned) -> String {
    now.date().to_string()
}

/// The next moment this alarm should ring, at or after `now`.
///
/// DST is handled by jiff's civil-time construction: a nonexistent local
/// time resolves forward (spring-forward gap) and an ambiguous one picks
/// the earlier occurrence, matching what people expect from an alarm.
pub fn next_trigger(alarm: &Alarm, now: &Zoned) -> Option<Zoned> {
    if !alarm.enabled {
        return None;
    }
    let (hour, minute) = parse_hhmm(&alarm.time)?;
    for day_offset in 0..=8i64 {
        let Ok(day) = now.date().checked_add(jiff::Span::new().days(day_offset)) else {
            continue;
        };
        // stored convention: 0 = Sunday .. 6 = Saturday; jiff counts from
        // Monday, so shift and wrap
        let stored_weekday = ((day.weekday().to_monday_zero_offset() as u8) + 1) % 7;
        let matches_repeat = match &alarm.repeat {
            AlarmRepeat::Once(date) => day.to_string() == *date,
            AlarmRepeat::Daily => true,
            AlarmRepeat::Weekdays(weekdays) => weekdays.contains(&stored_weekday),
        };
        if !matches_repeat {
            continue;
        }
        let candidate = day
            .at(hour, minute, 0, 0)
            .in_tz(now.time_zone().iana_name().unwrap_or("UTC"))
            .ok()?;
        // the same calendar day may already be past; day_offset 0 only
        // counts when the moment is still ahead
        if day_offset == 0 && candidate <= *now {
            // for a `Once` alarm the date matches exactly: it is past
            if let AlarmRepeat::Once(_) = alarm.repeat {
                return None;
            }
            continue;
        }
        if let AlarmRepeat::Once(date) = &alarm.repeat
            && day.to_string() != *date
        {
            continue;
        }
        return Some(candidate);
    }
    None
}

/// Whether the alarm should fire right now: today's scheduled moment
/// passed within the last 60 seconds and it has not fired today yet.
pub fn should_fire(alarm: &Alarm, now: &Zoned) -> bool {
    let today = date_key(now);
    if alarm.last_fired_date.as_deref() == Some(today.as_str()) {
        return false;
    }
    let Some((hour, minute)) = parse_hhmm(&alarm.time) else {
        return false;
    };
    let matches_today = match &alarm.repeat {
        AlarmRepeat::Once(date) => &today == date,
        AlarmRepeat::Daily => true,
        AlarmRepeat::Weekdays(weekdays) => {
            let stored_weekday = ((now.weekday().to_monday_zero_offset() as u8) + 1) % 7;
            weekdays.contains(&stored_weekday)
        }
    };
    if !matches_today {
        return false;
    }
    let Some(trigger) = now
        .date()
        .at(hour, minute, 0, 0)
        .in_tz(now.time_zone().iana_name().unwrap_or("UTC"))
        .ok()
    else {
        return false;
    };
    let seconds_late = now.timestamp().as_second() - trigger.timestamp().as_second();
    (0..=60).contains(&seconds_late)
}

/// A missed trigger that fired while the app was not running: catch up
/// within 10 minutes, once.
pub fn missed_recently(alarm: &Alarm, now: &Zoned) -> bool {
    if alarm.last_fired_date.as_deref() == Some(date_key(now).as_str()) {
        return false;
    }
    let Some((hour, minute)) = parse_hhmm(&alarm.time) else {
        return false;
    };
    let today = now.date();
    let scheduled = match today
        .at(hour, minute, 0, 0)
        .in_tz(now.time_zone().iana_name().unwrap_or("UTC"))
    {
        Ok(zoned) => zoned,
        Err(_) => return false,
    };
    let seconds_late = now.timestamp().as_second() - scheduled.timestamp().as_second();
    let stored_weekday = ((today.weekday().to_monday_zero_offset() as u8) + 1) % 7;
    let matches_day = match &alarm.repeat {
        AlarmRepeat::Once(date) => &date_key(now) == date,
        AlarmRepeat::Daily => true,
        AlarmRepeat::Weekdays(weekdays) => weekdays.contains(&stored_weekday),
    };
    matches_day && seconds_late > 60 && seconds_late <= 600
}

/// `HH:MM` display honoring the 12/24 hour preference.
pub fn format_time(now: &Zoned, use_24h: bool) -> String {
    let hour = now.hour();
    let minute = now.minute();
    if use_24h {
        format!("{hour:02}:{minute:02}")
    } else {
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let hour12 = hour % 12;
        let hour12 = if hour12 == 0 { 12 } else { hour12 };
        format!("{hour12}:{minute:02} {suffix}")
    }
}

/// `YYYY-MM-DD 周X` style date line.
pub fn format_date(now: &Zoned) -> String {
    let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [(now.weekday().to_monday_zero_offset() as usize) % 7];
    format!("{} {weekday}", now.date())
}

/// Days until (or, for one-shots past, since) an anniversary.
pub fn anniversary_days(date: &str, repeat_yearly: bool, now: &Zoned) -> Option<i64> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i16 = parts[0].parse().ok()?;
    let month: i8 = parts[1].parse().ok()?;
    let day: i8 = parts[2].parse().ok()?;
    let original = jiff::civil::date(year, month, day);
    if !repeat_yearly {
        return Some(
            now.date()
                .until(original)
                .map(|span| span.get_days() as i64)
                .unwrap_or(0),
        );
    }
    // next occurrence of (month, day) at or after today
    let this_year = jiff::civil::date(now.date().year(), month, day);
    if this_year >= now.date() {
        Some(
            now.date()
                .until(this_year)
                .map(|span| span.get_days() as i64)
                .unwrap_or(0),
        )
    } else {
        let next_year = jiff::civil::date(now.date().year() + 1, month, day);
        Some(
            now.date()
                .until(next_year)
                .map(|span| span.get_days() as i64)
                .unwrap_or(0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Alarm;
    use jiff::civil::date;

    fn fixed_now() -> Zoned {
        // 2026-10-11 09:30 Sunday, UTC (deterministic in tests)
        date(2026, 10, 11).at(9, 30, 0, 0).in_tz("UTC").unwrap()
    }

    fn alarm(time: &str, repeat: AlarmRepeat) -> Alarm {
        Alarm {
            id: "a1".into(),
            title: "测试".into(),
            time: time.into(),
            repeat,
            enabled: true,
            sound: true,
            last_fired_date: None,
            created_at: 0,
        }
    }

    #[test]
    fn parses_valid_hhmm_only() {
        assert_eq!(parse_hhmm("07:05"), Some((7, 5)));
        assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("7:5"), None);
        assert_eq!(parse_hhmm("abc"), None);
    }

    #[test]
    fn daily_alarm_triggers_today_or_tomorrow() {
        let now = fixed_now();
        // 10:00 today is in the future
        let a = alarm("10:00", AlarmRepeat::Daily);
        let trigger = next_trigger(&a, &now).unwrap();
        assert_eq!(trigger.date(), date(2026, 10, 11));
        assert_eq!(trigger.hour(), 10);
        // 09:00 today is past -> tomorrow
        let b = alarm("09:00", AlarmRepeat::Daily);
        let trigger = next_trigger(&b, &now).unwrap();
        assert_eq!(trigger.date(), date(2026, 10, 12));
        // disabled alarms never trigger
        let mut c = alarm("10:00", AlarmRepeat::Daily);
        c.enabled = false;
        assert!(next_trigger(&c, &now).is_none());
    }

    #[test]
    fn once_alarm_fires_on_its_date_only() {
        let now = fixed_now();
        // a future-dated one-shot still resolves
        let a = alarm("20:00", AlarmRepeat::Once("2026-10-11".into()));
        let trigger = next_trigger(&a, &now).unwrap();
        assert_eq!(trigger.hour(), 20);
        // same date but a past time -> never (the moment is gone)
        let b = alarm("09:00", AlarmRepeat::Once("2026-10-11".into()));
        assert!(next_trigger(&b, &now).is_none());
        let c = alarm("10:00", AlarmRepeat::Once("2026-10-10".into()));
        assert!(next_trigger(&c, &now).is_none());
    }

    #[test]
    fn weekday_repeat_respects_the_day_list() {
        let now = fixed_now(); // Sunday Oct 11 2026
        let a = alarm("20:00", AlarmRepeat::Weekdays(vec![0])); // Sundays only
        let trigger = next_trigger(&a, &now).unwrap();
        assert_eq!(trigger.date(), date(2026, 10, 11));
        let b = alarm("20:00", AlarmRepeat::Weekdays(vec![1])); // Mondays
        let trigger = next_trigger(&b, &now).unwrap();
        assert_eq!(trigger.date(), date(2026, 10, 12));
    }

    #[test]
    fn should_fire_within_the_grace_window_once_per_day() {
        let mut a = alarm("09:29", AlarmRepeat::Daily);
        // fired 29 seconds ago
        let now = date(2026, 10, 11).at(9, 29, 29, 0).in_tz("UTC").unwrap();
        assert!(should_fire(&a, &now));
        // mark as fired today -> no second fire
        a.last_fired_date = Some("2026-10-11".into());
        assert!(!should_fire(&a, &now));
        // too late (> 60s) and not fired: startup catch-up territory
        a.last_fired_date = None;
        let late = date(2026, 10, 11).at(9, 35, 0, 0).in_tz("UTC").unwrap();
        assert!(!should_fire(&a, &late));
        assert!(missed_recently(&a, &late), "within the 10min catch-up");
        let very_late = date(2026, 10, 11).at(12, 0, 0, 0).in_tz("UTC").unwrap();
        assert!(!missed_recently(&a, &very_late));
    }

    #[test]
    fn time_formats_both_styles() {
        let now = date(2026, 10, 11).at(15, 5, 0, 0).in_tz("UTC").unwrap();
        assert_eq!(format_time(&now, true), "15:05");
        assert_eq!(format_time(&now, false), "3:05 PM");
    }

    #[test]
    fn anniversary_counts_days() {
        let now = fixed_now(); // 2026-10-11
        // same month/day this year -> today (0 days)
        assert_eq!(anniversary_days("2020-10-11", true, &now), Some(0));
        // upcoming this year
        assert_eq!(anniversary_days("2020-12-25", true, &now), Some(75));
        // passed this year -> next year's occurrence: Oct 11 -> Jan 1 is 82 days
        assert_eq!(anniversary_days("2020-01-01", true, &now), Some(82));
        // non-repeating: days since (negative means past)
        assert_eq!(anniversary_days("2026-10-01", false, &now), Some(-10));
    }
}
