//! The time side of a post, as K2 Press's `schedule.py`: parsing dates, "is this due?", free
//! slots from the configured posting times, and the calendar month grid. Local time.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde_json::Value;

use crate::config::Config;
use crate::library::{Entry, s};

/// `2026-10-15T09:00`, `2026-10-15 09:00`, `2026-10-15T09:00:00`, or a bare date (09:00).
pub fn parse_when(v: &str) -> Option<NaiveDateTime> {
    let v = v.trim().trim_end_matches('Z');
    let v = v.split('+').next().unwrap_or(v);
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(v, f) {
            return Some(d);
        }
    }
    NaiveDate::parse_from_str(v, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(9, 0, 0))
}

pub fn iso(d: NaiveDateTime) -> String {
    d.format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// A scheduled entry whose time has come.
pub fn is_due(e: &Entry, at: NaiveDateTime) -> bool {
    s(e, "status") == "scheduled" && parse_when(s(e, "scheduled_at")).is_some_and(|w| w <= at)
}

pub struct Slots {
    pub times: Vec<NaiveTime>,
    pub days: Vec<Weekday>,
    pub auto_publish: bool,
}

pub fn slots(cfg: &Config) -> Slots {
    let strs = |k: &str| -> Vec<String> {
        cfg.at(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    };
    let mut times: Vec<NaiveTime> = strs("schedule.times").iter().filter_map(|t| NaiveTime::parse_from_str(t.trim(), "%H:%M").ok()).collect();
    if times.is_empty() {
        times = [9, 13, 18].iter().filter_map(|h| NaiveTime::from_hms_opt(*h, 0, 0)).collect();
    }
    times.sort();
    let mut days: Vec<Weekday> = strs("schedule.days").iter().filter_map(|d| d.trim().get(..3).and_then(|d| d.parse::<Weekday>().ok())).collect();
    if days.is_empty() {
        days = vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri];
    }
    let auto_publish = cfg.at("schedule.auto_publish").and_then(Value::as_bool).unwrap_or(true);
    Slots { times, days, auto_publish }
}

/// The next `count` free posting slots after `from` (taken = already scheduled times).
pub fn next_slots(cfg: &Config, entries: &[Entry], count: usize, from: NaiveDateTime) -> Vec<NaiveDateTime> {
    let sl = slots(cfg);
    let taken: Vec<NaiveDateTime> = entries
        .iter()
        .filter(|e| matches!(s(e, "status"), "scheduled" | "cloud_scheduled" | "published"))
        .filter_map(|e| parse_when(s(e, "scheduled_at")))
        .collect();
    let mut out = Vec::new();
    let mut day = from.date();
    for _ in 0..400 {
        if sl.days.contains(&day.weekday()) {
            for t in &sl.times {
                let at = day.and_time(*t);
                if at > from && !taken.contains(&at) && !out.contains(&at) {
                    out.push(at);
                    if out.len() >= count {
                        return out;
                    }
                }
            }
        }
        day += Duration::days(1);
    }
    out
}

/// The six-week grid (Monday first) covering `year`-`month`.
pub fn month_grid(year: i32, month: u32) -> Vec<NaiveDate> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else { return Vec::new() };
    let start = first - Duration::days(i64::from(first.weekday().num_days_from_monday()));
    (0..42).map(|i| start + Duration::days(i)).collect()
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let m = year * 12 + month as i32 - 1 + delta;
    (m.div_euclid(12), (m.rem_euclid(12) + 1) as u32)
}
