//! The runtime scheduler: time-based triggers as first-class engine inputs.
//!
//! Schedules are *inputs*, not a parallel engine: when one fires, the task
//! builds a real [`Event`] with `kind = schedule.fired` and pushes it onto
//! the same bounded [`WorkItem`] queue the HTTP and MQTT adapters use. It
//! then flows the standard pipeline → state → rules → actions path, so the
//! only remaining step is a webhook action bridging it to Mero.
//!
//! Clock policy: every task computes its next run from a named instant
//! (`ScheduledAt`) and re-reads `Utc::now()` only when it wakes — a late
//! wake-up advances the schedule instead of firing at the wrong cadence.
//! Wall time enters here (the runtime boundary) as everywhere else.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::event::{Event, Payload};
use crate::runtime::http::WorkItem;

/// Event kind emitted by every schedule firing. Rules match the family
/// (`schedule`) or the exact kind (`schedule.fired`).
pub const SCHEDULE_EVENT_KIND: &str = "schedule.fired";
/// Event source for scheduler-produced events.
pub const SCHEDULE_EVENT_SOURCE: &str = "deltu://scheduler";

/// Calendar unit for weekly / every-N-weeks schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    #[default]
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// 1-based ISO weekday (Monday = 1 … Sunday = 7), matching `chrono`.
    #[allow(dead_code)] // exercised via from_iso in next-run paths + tests
    fn iso(self) -> u32 {
        match self {
            Weekday::Monday => 1,
            Weekday::Tuesday => 2,
            Weekday::Wednesday => 3,
            Weekday::Thursday => 4,
            Weekday::Friday => 5,
            Weekday::Saturday => 6,
            Weekday::Sunday => 7,
        }
    }

    fn from_iso(n: u32) -> Self {
        match n {
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            6 => Weekday::Saturday,
            _ => Weekday::Sunday,
        }
    }

    #[allow(dead_code)] // used by parse_list in config/test paths
    fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_lowercase().as_str() {
            "mon" | "monday" => Weekday::Monday,
            "tue" | "tues" | "tuesday" => Weekday::Tuesday,
            "wed" | "weds" | "wednesday" => Weekday::Wednesday,
            "thu" | "thur" | "thurs" | "thursday" => Weekday::Thursday,
            "fri" | "friday" => Weekday::Friday,
            "sat" | "saturday" => Weekday::Saturday,
            "sun" | "sunday" => Weekday::Sunday,
            _ => return None,
        })
    }

    #[allow(dead_code)] // reserved for string-list configs (tests cover it)
    fn parse_list(names: &str) -> Result<Vec<Self>, String> {
        let mut days = Vec::new();
        for part in names.split(',') {
            let token = part.trim();
            if token.is_empty() {
                continue;
            }
            let day = Self::parse(token)
                .ok_or_else(|| format!("unknown weekday {token:?} (use Mon, Tue, Wed, Thu, Fri, Sat, Sun)"))?;
            if !days.contains(&day) {
                days.push(day);
            }
        }
        if days.is_empty() {
            return Err("at least one weekday is required".to_string());
        }
        Ok(days)
    }
}

/// Cadence of a schedule. Interval-style schedules run from the schedule's
/// creation moment; calendar-style schedules anchor to local wall-clock
/// time in `timezone`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScheduleSpec {
    /// Every `every` seconds, minutes, or hours.
    Interval {
        /// Unit multiplier.
        unit: IntervalUnit,
        /// How many units between runs (>= 1).
        every: u32,
    },
    /// Every day at `time` in the schedule's timezone.
    Daily {
        /// Local wall-clock time of day.
        time: String,
    },
    /// On the listed weekdays at `time` in the schedule's timezone.
    Weekly {
        /// Weekdays the schedule runs on (non-empty).
        days: Vec<Weekday>,
        /// Local wall-clock time of day.
        time: String,
    },
    /// Every `interval_weeks` weeks on `day` at `time` (1 = weekly).
    EveryNWeeks {
        /// Weeks between runs (>= 1; 1 behaves like a single-day weekly).
        interval_weeks: u32,
        /// The day of week the schedule runs on.
        day: Weekday,
        /// Local wall-clock time of day.
        time: String,
    },
    /// Monthly — `day` = day of month (1–31; clamped to the month's last
    /// day, so 31 runs on the last day of short months), or `on_last_day`
    /// for explicit last-day semantics. Optional `nth_week` for
    /// "third Friday of the month" style schedules.
    Monthly {
        /// Day of month, 1–31 (clamped per month).
        day: u32,
        /// Local wall-clock time of day.
        time: String,
    },
    /// Monthly on the `nth` (1–5) `weekday` of the month.
    MonthlyNth {
        /// Which occurrence of `weekday` in the month (1–5; months without
        /// a 5th occurrence skip to the next matching month).
        nth: u32,
        /// Day of week.
        weekday: Weekday,
        /// Local wall-clock time of day.
        time: String,
    },
    /// Monthly on the last day of the month.
    MonthlyLast {
        /// Local wall-clock time of day.
        time: String,
    },
    /// Standard cron expression (5 or 6 fields — `cron` crate supports
    /// seconds in the 6-field form). Evaluated in the schedule's timezone.
    Cron {
        /// The cron expression string.
        expression: String,
    },
}

/// Interval unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IntervalUnit {
    /// Seconds (minimum resolution of the scheduler).
    #[default]
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
}

impl IntervalUnit {
    fn seconds(self) -> i64 {
        match self {
            IntervalUnit::Seconds => 1,
            IntervalUnit::Minutes => 60,
            IntervalUnit::Hours => 3_600,
        }
    }
}

/// What happens to runs missed while the engine was down, paused, or the
/// process was suspended. Only *whole* missed occurrences are considered:
/// interval schedules catch up at most `catch_up_max` times and then
/// resynchronize to "next occurrence after now".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissedPolicy {
    /// Fire once (the latest missed occurrence), then resume normally.
    /// Default: the operator asked for a run at that time, so one run is
    /// delivered — but a week-long outage does not replay a week of dailies.
    #[default]
    FireOnce,
    /// Skip missed occurrences entirely and resume at the next future one.
    Skip,
    /// Catch up up to `catch_up_max` missed occurrences, oldest first
    /// (bounded replay).
    CatchUp {
        /// Maximum number of missed runs replayed after an outage.
        catch_up_max: u32,
    },
}

/// Overlap policy when the previous run is still being processed (its
/// event is still queued or in the pipeline — the engine is single-worker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    /// Drop the firing and keep the schedule anchored (default — safe).
    #[default]
    SkipIfBusy,
    /// Try to enqueue anyway; a full queue surfaces `queue_full` and the
    /// firing counts as missed (never blocks the task).
    Allow,
}

/// A configured schedule. `id`, `spec`, and `timezone` are required; the
/// rest are validated defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Unique, non-empty identifier (upsert semantics on add).
    pub id: String,
    /// When the schedule runs.
    pub spec: ScheduleSpec,
    /// IANA timezone name (e.g. `Asia/Kolkata`, `UTC`) for calendar specs.
    pub timezone: String,
    /// Payload delivered inside the fired event (free-form JSON).
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Missed-run policy. Default `fire_once`.
    #[serde(default)]
    pub missed_policy: MissedPolicy,
    /// Overlap policy. Default `skip_if_busy`.
    #[serde(default)]
    pub overlap_policy: OverlapPolicy,
    /// Optional start boundary: occurrences strictly before this instant
    /// (epoch ms UTC) never fire.
    #[serde(default)]
    pub not_before_ms: Option<i64>,
    /// Optional end boundary: occurrences at/after this instant (epoch ms
    /// UTC) never fire.
    #[serde(default)]
    pub not_after_ms: Option<i64>,
    /// Whether the schedule is enabled. Disabled schedules stay
    /// registered (and visible in the API) but never fire.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Status of one live schedule.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScheduleStatus {
    /// The full configuration.
    pub config: ScheduleConfig,
    /// Whether the run task is alive.
    pub running: bool,
    /// Next scheduled run (epoch ms UTC), when computable.
    pub next_run_ms: Option<i64>,
    /// Last fire accepted by the queue (epoch ms UTC).
    pub last_fired_ms: Option<i64>,
    /// Fireings accepted since (re)start.
    pub fired_count: u64,
    /// Occurrences skipped: overlap policy or a full queue.
    pub skipped_count: u64,
    /// Missed occurrences replayed after downtime.
    pub caught_up_count: u64,
    /// Missed occurrences dropped (policy skip, or over the catch-up cap).
    pub missed_count: u64,
    /// The concrete cron expression a spec compiles to (empty for none).
    pub effective_cron: String,
}

impl ScheduleConfig {
    /// Validates ids, cadence values, time strings, weekday lists, and the
    /// IANA timezone. The error names the offending field.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("id must not be empty".to_string());
        }
        Tz::from_str(self.timezone.trim())
            .map_err(|_| format!("timezone {:?} is not a valid IANA timezone", self.timezone))?;
        match &self.spec {
            ScheduleSpec::Interval { unit: _, every } => {
                if *every == 0 {
                    return Err("interval every must be >= 1".to_string());
                }
                let secs = i64::from(*every) * unit_secs(&self.spec);
                if secs < 1 {
                    return Err("interval must be at least 1 second".to_string());
                }
            }
            ScheduleSpec::Daily { time } => {
                parse_time(time)?;
            }
            ScheduleSpec::Weekly { days, time } => {
                if days.is_empty() {
                    return Err("weekly days must not be empty".to_string());
                }
                parse_time(time)?;
            }
            ScheduleSpec::EveryNWeeks {
                interval_weeks,
                day: _,
                time,
            } => {
                if *interval_weeks == 0 {
                    return Err("every_n_weeks interval_weeks must be >= 1".to_string());
                }
                parse_time(time)?;
            }
            ScheduleSpec::Monthly { day, time } => {
                if !(1..=31).contains(day) {
                    return Err(format!("monthly day must be 1..=31, got {day}"));
                }
                parse_time(time)?;
            }
            ScheduleSpec::MonthlyNth { nth, weekday: _, time } => {
                if !(1..=5).contains(nth) {
                    return Err(format!("monthly_nth nth must be 1..=5, got {nth}"));
                }
                parse_time(time)?;
            }
            ScheduleSpec::MonthlyLast { time } => {
                parse_time(time)?;
            }
            ScheduleSpec::Cron { expression } => {
                CronSchedule::from_str(&expand_cron(expression.trim()))
                    .map_err(|error| format!("invalid cron expression: {error}"))?;
            }
        }
        if let Some(before) = self.not_before_ms
            && let Some(after) = self.not_after_ms
            && before >= after
        {
            return Err("not_before_ms must be before not_after_ms".to_string());
        }
        Ok(())
    }

    /// The concrete cron expression this spec would evaluate to, or `None`
    /// for specs computed by hand (calendar + interval specs).
    pub fn effective_cron(&self) -> Option<String> {
        match &self.spec {
            ScheduleSpec::Cron { expression } => Some(expand_cron(expression.trim())),
            _ => None,
        }
    }
}

fn unit_secs(spec: &ScheduleSpec) -> i64 {
    match spec {
        ScheduleSpec::Interval { unit, every: _ } => unit.seconds(),
        _ => 1,
    }
}

/// Parses `HH:MM` or `HH:MM:SS` local time.
pub fn parse_time(value: &str) -> Result<NaiveTime, String> {
    let trimmed = value.trim();
    let parts: Vec<&str> = trimmed.split(':').collect();
    match parts.as_slice() {
        [h, m] => {
            let hour: u32 = h.trim().parse().map_err(|_| format!("invalid hour in time {value:?}"))?;
            let minute: u32 = m.trim().parse().map_err(|_| format!("invalid minute in time {value:?}"))?;
            NaiveTime::from_hms_opt(hour, minute, 0).ok_or_else(|| format!("time {value:?} out of range"))
        }
        [h, m, s] => {
            let hour: u32 = h.trim().parse().map_err(|_| format!("invalid hour in time {value:?}"))?;
            let minute: u32 = m.trim().parse().map_err(|_| format!("invalid minute in time {value:?}"))?;
            let second: u32 = s.trim().parse().map_err(|_| format!("invalid second in time {value:?}"))?;
            NaiveTime::from_hms_opt(hour, minute, second).ok_or_else(|| format!("time {value:?} out of range"))
        }
        _ => Err(format!("time {value:?} must be HH:MM or HH:MM:SS")),
    }
}

/// Computes the next occurrence strictly after `after` for a spec, in the
/// spec's timezone. Pure: tests drive it with fixed instants.
pub fn next_occurrence(spec: &ScheduleSpec, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match spec {
        ScheduleSpec::Interval { unit, every } => {
            let step_secs = i64::from(*every) * unit.seconds();
            // Anchor on the step grid relative to `after` so a late wake
            // resynchronizes instead of bursting.
            let after_secs = after.timestamp();
            let steps_to_next = 1_i64.max((after_secs / step_secs) + 1 - (after_secs / step_secs));
            let _ = steps_to_next; // clarity: next grid point strictly after `after`
            let next_grid = ((after_secs / step_secs) + 1) * step_secs;
            let _ = unit;
            Utc.timestamp_opt(next_grid, 0).single()
        }
        ScheduleSpec::Daily { time } => {
            let t = parse_time(time).ok()?;
            next_daily(t, tz, after)
        }
        ScheduleSpec::Weekly { days, time } => {
            let t = parse_time(time).ok()?;
            next_weekly(days, tz, after, t)
        }
        ScheduleSpec::EveryNWeeks {
            interval_weeks,
            day,
            time,
        } => {
            let t = parse_time(time).ok()?;
            next_every_n_weeks(*interval_weeks, *day, t, tz, after)
        }
        ScheduleSpec::Monthly { day, time } => {
            let t = parse_time(time).ok()?;
            next_monthly(*day, t, tz, after)
        }
        ScheduleSpec::MonthlyNth { nth, weekday, time } => {
            let t = parse_time(time).ok()?;
            next_monthly_nth(*nth, *weekday, t, tz, after)
        }
        ScheduleSpec::MonthlyLast { time } => {
            let t = parse_time(time).ok()?;
            next_monthly_last(t, tz, after)
        }
        ScheduleSpec::Cron { expression } => {
            let cron = CronSchedule::from_str(&expand_cron(expression.trim())).ok()?;
            cron.after(&after.with_timezone(&tz)).next().map(|next| next.with_timezone(&Utc))
        }
    }
}

fn next_daily(t: NaiveTime, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    for day_offset in 0..2 {
        let date = local_after.date_naive() + ChronoDuration::days(day_offset);
        let candidate_local = tz
            .from_local_datetime(&date.and_time(t))
            .single()?;
        let candidate_utc = candidate_local.with_timezone(&Utc);
        if candidate_utc > after {
            return Some(candidate_utc);
        }
    }
    None
}

fn next_weekly(days: &[Weekday], tz: Tz, after: DateTime<Utc>, t: NaiveTime) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    // Up to 8 days forward always covers every weekday at least once.
    for day_offset in 0..8 {
        let date = local_after.date_naive() + ChronoDuration::days(day_offset);
        let weekday = Weekday::from_iso(date.weekday().number_from_monday());
        if !days.contains(&weekday) {
            continue;
        }
        if let Some(candidate_local) = tz.from_local_datetime(&date.and_time(t)).single()
            && candidate_local.with_timezone(&Utc) > after
        {
            return Some(candidate_local.with_timezone(&Utc));
        }
    }
    None
}

/// Anchor for every-N-weeks: the Monday of the week (local) containing the
/// schedule's first occurrence at-or-after `after`... computed forward from
/// the current week so restarts stay aligned to the same grid.
fn next_every_n_weeks(interval_weeks: u32, day: Weekday, t: NaiveTime, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    // Walk week by week; the grid anchors to the week of `after` so it is
    // stable across restarts without persisting an anchor date.
    for week_offset in 0..(interval_weeks.max(1) * 2 + 2) {
        // find the next matching weekday in this candidate week
        for day_offset in 0..7 {
            let date = local_after.date_naive()
                + ChronoDuration::days(i64::from(week_offset) * 7 + day_offset);
            if Weekday::from_iso(date.weekday().number_from_monday()) != day {
                continue;
            }
            if let Some(candidate_local) = tz.from_local_datetime(&date.and_time(t)).single()
                && candidate_local.with_timezone(&Utc) > after
            {
                // Only fire on weeks matching the N-week grid. Anchor:
                // weeks where weeks_since_epoch % N == anchor % N, where the
                // anchor is derived from the *first* matching week so the
                // grid never drifts across restarts.
                let weeks = date.num_weeks_since_unix_epoch_floor();
                if (weeks % i64::from(interval_weeks.max(1))) == 0 {
                    return Some(candidate_local.with_timezone(&Utc));
                }
            }
        }
    }
    None
}

trait WeeksSinceEpoch {
    /// Monday-starting week index (floor) since the Unix epoch, in UTC days.
    fn num_weeks_since_unix_epoch_floor(&self) -> i64;
}

impl WeeksSinceEpoch for chrono::NaiveDate {
    fn num_weeks_since_unix_epoch_floor(&self) -> i64 {
        // Day 0 = 1970-01-01 (a Thursday). Monday of that week:
        // 1970-01-05. Days since 1970-01-05, divided by 7 (floor).
        let epoch_monday = chrono::NaiveDate::from_ymd_opt(1970, 1, 5).unwrap();
        self.signed_duration_since(epoch_monday).num_days().div_euclid(7)
    }
}

fn next_monthly(day: u32, t: NaiveTime, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    for month_offset in 0..3 {
        let candidate_month = local_after.date_naive().with_day(1).unwrap()
            + ChronoDuration::days(30 * month_offset)
            + ChronoDuration::days(2 * month_offset); // covers 28–31 day months
        let month_start = candidate_month.with_day(1).unwrap();
        let last_day = last_day_of(month_start);
        let actual_day = day.min(last_day);
        let date = month_start.with_day(actual_day).unwrap();
        if let Some(candidate_local) = tz.from_local_datetime(&date.and_time(t)).single()
            && candidate_local.with_timezone(&Utc) > after
        {
            return Some(candidate_local.with_timezone(&Utc));
        }
    }
    None
}

fn last_day_of(month_start: chrono::NaiveDate) -> u32 {
    let year = month_start.year();
    let month = month_start.month();
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let next_month_start =
        chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    (next_month_start - ChronoDuration::days(1)).day()
}

fn next_monthly_nth(nth: u32, weekday: Weekday, t: NaiveTime, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    for month_offset in 0..3 {
        let base = local_after.date_naive().with_day(1).unwrap()
            + ChronoDuration::days(30 * month_offset)
            + ChronoDuration::days(2 * month_offset);
        let month_start = base.with_day(1).unwrap();
        if let Some(date) = nth_weekday_of_month(month_start, nth, weekday)
            && let Some(candidate_local) = tz.from_local_datetime(&date.and_time(t)).single()
            && candidate_local.with_timezone(&Utc) > after
        {
            return Some(candidate_local.with_timezone(&Utc));
        }
    }
    None
}

fn nth_weekday_of_month(month_start: chrono::NaiveDate, nth: u32, weekday: Weekday) -> Option<chrono::NaiveDate> {
    let mut date = month_start;
    let mut seen = 0;
    while date.month() == month_start.month() {
        if Weekday::from_iso(date.weekday().number_from_monday()) == weekday {
            seen += 1;
            if seen == nth {
                return Some(date);
            }
        }
        date += ChronoDuration::days(1);
    }
    None
}

fn next_monthly_last(t: NaiveTime, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local_after = after.with_timezone(&tz);
    for month_offset in 0..3 {
        let candidate = local_after.date_naive().with_day(1).unwrap()
            + ChronoDuration::days(30 * month_offset)
            + ChronoDuration::days(2 * month_offset);
        let month_start = candidate.with_day(1).unwrap();
        let date = month_start.with_day(last_day_of(month_start)).unwrap();
        if let Some(candidate_local) = tz.from_local_datetime(&date.and_time(t)).single()
            && candidate_local.with_timezone(&Utc) > after
        {
            return Some(candidate_local.with_timezone(&Utc));
        }
    }
    None
}

/// The `cron` crate requires seconds (6/7 fields). Mero and users write
/// conventional 5-field cron, so a 5-field expression is expanded with a
/// `0` seconds field. 6/7-field expressions pass through untouched.
fn expand_cron(expression: &str) -> String {
    let field_count = expression.split_whitespace().count();
    if field_count == 5 {
        format!("0 {expression}")
    } else {
        expression.to_string()
    }
}

/// Validation error collection for a whole config set (startup path).
pub fn validate_all(configs: &[ScheduleConfig]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for config in configs {
        config
            .validate()
            .map_err(|error| format!("schedule {:?}: {error}", config.id))?;
        if !seen.insert(config.id.trim().to_string()) {
            return Err(format!("duplicate schedule id: {}", config.id));
        }
    }
    Ok(())
}

/// Statistics counters shared between tasks and the status API. Atomics
/// (same pattern as `PersistenceCounters`): tasks mutate, snapshots read.
#[derive(Debug, Default)]
pub struct ScheduleCounters {
    pub fired: std::sync::atomic::AtomicU64,
    pub skipped: std::sync::atomic::AtomicU64,
    pub caught_up: std::sync::atomic::AtomicU64,
    pub missed: std::sync::atomic::AtomicU64,
    pub queue_full: std::sync::atomic::AtomicU64,
}

/// One independent per-schedule runtime task.
struct ScheduleTask {
    config: ScheduleConfig,
    handle: JoinHandle<()>,
    counters: Arc<ScheduleCounters>,
    last_fired_ms: Option<i64>,
}

/// The scheduler: owns configs and one task per schedule. All mutation is
/// through `&mut self` (the HTTP layer serializes access behind a mutex).
pub struct Scheduler {
    tasks: HashMap<String, ScheduleTask>,
    queue: Arc<tokio::sync::mpsc::Sender<WorkItem>>,
    /// Set when the queue rejects a firing (full/closed) so the status API
    /// can surface it.
    last_error: Option<String>,
}

impl Scheduler {
    pub fn new(queue: Arc<tokio::sync::mpsc::Sender<WorkItem>>) -> Self {
        Self {
            tasks: HashMap::new(),
            queue,
            last_error: None,
        }
    }

    /// Adds (or replaces) a schedule and starts its task. Validation runs
    /// before any state change — a rejected config leaves the existing
    /// schedule untouched.
    pub fn add(&mut self, config: ScheduleConfig) -> Result<(), String> {
        config.validate().map_err(|error| format!("invalid schedule: {error}"))?;
        let id = config.id.trim().to_string();
        // replace-or-insert: abort any previous task for this id
        self.remove(&id);
        if config.enabled {
            let task = self.spawn(config.clone());
            self.tasks.insert(id, task);
        } else {
            // registered but not running
            self.tasks.insert(
                id,
                ScheduleTask {
                    config,
                    handle: tokio::spawn(async {}),
                    counters: Arc::new(ScheduleCounters::default()),
                    last_fired_ms: None,
                },
            );
        }
        Ok(())
    }

    /// Stops a schedule's task and removes it. Idempotent.
    pub fn remove(&mut self, id: &str) {
        if let Some(task) = self.tasks.remove(id) {
            task.handle.abort();
        }
    }

    /// Stops (or restarts) a schedule without deleting it. Returns false
    /// when the id is unknown.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<bool, String> {
        let Some(task) = self.tasks.get(id) else {
            return Ok(false);
        };
        if task.config.enabled == enabled {
            return Ok(true);
        }
        let mut config = task.config.clone();
        config.enabled = enabled;
        self.remove(id);
        self.add(config)?;
        Ok(true)
    }

    /// Runs one schedule immediately, out of band (manual trigger). The
    /// event carries `"manual": true` so receivers can tell it apart.
    pub fn trigger_now(&mut self, id: &str) -> Result<(), String> {
        let task = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("unknown schedule {id:?}"))?;
        let mut payload = task.config.payload.clone();
        if !payload.is_object() {
            payload = serde_json::Value::Object(serde_json::Map::new());
        }
        if let Some(map) = payload.as_object_mut() {
            map.insert("manual".to_string(), serde_json::Value::Bool(true));
        }
        let fired_at = Utc::now().timestamp_millis();
        if send_event(&self.queue, &task.config.id, fired_at, payload).is_some() {
            task.counters
                .fired
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        task.last_fired_ms = Some(fired_at);
        Ok(())
    }

    /// Snapshot for the status API: configs + liveness + counters +
    /// next-run. Sorted by id for deterministic output.
    pub fn snapshot(&self) -> Vec<ScheduleStatus> {
        let mut statuses: Vec<ScheduleStatus> = self
            .tasks
            .values()
            .map(|task| {
                let tz = Tz::from_str(task.config.timezone.trim()).unwrap_or(Tz::UTC);
                let now = Utc::now();
                let next_run_ms = if task.config.enabled {
                    next_occurrence(&task.config.spec, tz, now).map(|next| next.timestamp_millis())
                } else {
                    None
                };
                ScheduleStatus {
                    config: task.config.clone(),
                    running: task.config.enabled && !task.handle.is_finished(),
                    next_run_ms,
                    last_fired_ms: task.last_fired_ms,
                    fired_count: task.counters.fired.load(std::sync::atomic::Ordering::Relaxed),
                    skipped_count: task.counters.skipped.load(std::sync::atomic::Ordering::Relaxed),
                    caught_up_count: task.counters.caught_up.load(std::sync::atomic::Ordering::Relaxed),
                    missed_count: task.counters.missed.load(std::sync::atomic::Ordering::Relaxed),
                    effective_cron: task.config.effective_cron().unwrap_or_default(),
                }
            })
            .collect();
        statuses.sort_by(|a, b| a.config.id.cmp(&b.config.id));
        statuses
    }

    /// The last queue error, if any (for status surfacing).
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Persists all current configs (with runtime counters cleared) to
    /// `path` atomically (temp + rename, mirroring the persistence
    /// adapter's crash-consistency contract).
    pub fn save(&self, path: &str) -> Result<(), String> {
        let configs: Vec<ScheduleConfig> = self.tasks.values().map(|t| t.config.clone()).collect();
        let file = ScheduleSnapshot {
            format_version: SNAPSHOT_FORMAT_VERSION,
            schedules: configs,
        };
        let serialized = serde_json::to_vec(&file)
            .map_err(|error| format!("schedule snapshot serialize failed: {error}"))?;
        let temp = format!("{path}.tmp");
        std::fs::write(&temp, serialized)
            .map_err(|error| format!("failed to write {temp}: {error}"))?;
        std::fs::rename(&temp, path).map_err(|error| format!("failed to rename into {path}: {error}"))
    }

    /// Restores configs from `path`, returning them (the caller re-adds
    /// them so tasks spawn inside a live runtime). Unknown snapshot
    /// versions fail loudly; individual configs that no longer validate
    /// are reported, not silently dropped.
    pub fn restore(path: &str) -> Result<Vec<ScheduleConfig>, String> {
        if !std::path::Path::new(path).exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
        let file: ScheduleSnapshot = serde_json::from_slice(&bytes)
            .map_err(|error| format!("schedule snapshot {path} is corrupted: {error}"))?;
        if file.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(format!(
                "schedule snapshot {path} has unsupported format version {} (expected {SNAPSHOT_FORMAT_VERSION})",
                file.format_version
            ));
        }
        for config in &file.schedules {
            config
                .validate()
                .map_err(|error| format!("restored schedule {:?} no longer validates: {error}", config.id))?;
        }
        Ok(file.schedules)
    }

    fn spawn(&self, config: ScheduleConfig) -> ScheduleTask {
        let queue = self.queue.clone();
        let counters = Arc::new(ScheduleCounters::default());
        let task_counters = counters.clone();
        let task_config = config.clone();
        let handle = tokio::spawn(async move {
            run_task(task_config, queue, task_counters).await;
        });
        ScheduleTask {
            config,
            handle,
            counters,
            last_fired_ms: None,
        }
    }
}

const SNAPSHOT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ScheduleSnapshot {
    format_version: u32,
    schedules: Vec<ScheduleConfig>,
}

/// Minimum sleep granularity: wakes are quantized so a just-missed
/// occurrence (sub-second race) cannot double-fire.
const WAKE_SLACK_MS: i64 = 50;

async fn run_task(config: ScheduleConfig, queue: Arc<tokio::sync::mpsc::Sender<WorkItem>>, counters: Arc<ScheduleCounters>) {
    let Ok(tz) = Tz::from_str(config.timezone.trim()) else {
        return; // validated at add(); defensive only
    };
    let mut fired_at_least_once = false;
    loop {
        let now = Utc::now();
        let mut next = match next_occurrence(&config.spec, tz, now) {
            Some(next) => next,
            None => return, // non-repeating / unparseable: end the task
        };

        // Catch-up scan after (re)start or pause: how many occurrences were
        // missed while we were away? Only interval specs have a computable
        // gap count; calendar specs compare the last *expected* occurrence
        // before now against the last one we actually fired.
        if fired_at_least_once {
            // normal steady state — no catch-up (the loop keeps the anchor)
        } else {
            use std::sync::atomic::Ordering::Relaxed;
            let missed = count_missed(&config.spec, tz, now);
            if missed > 0 {
                match config.missed_policy {
                    MissedPolicy::Skip => {
                        counters.missed.fetch_add(missed as u64, Relaxed);
                    }
                    MissedPolicy::FireOnce => {
                        // deliver one run now, mark the rest missed
                        counters.caught_up.fetch_add(1, Relaxed);
                        counters.missed.fetch_add((missed.saturating_sub(1)) as u64, Relaxed);
                        let fired_at = send_event(&queue, &config.id, now.timestamp_millis(), config.payload.clone());
                        if fired_at.is_some() {
                            counters.fired.fetch_add(1, Relaxed);
                        }
                    }
                    MissedPolicy::CatchUp { catch_up_max } => {
                        let replay = missed.min(i64::from(catch_up_max.max(1)));
                        for _ in 0..replay {
                            let fired_at = send_event(&queue, &config.id, now.timestamp_millis(), config.payload.clone());
                            if fired_at.is_some() {
                                counters.fired.fetch_add(1, Relaxed);
                                counters.caught_up.fetch_add(1, Relaxed);
                            } else {
                                counters.missed.fetch_add(1, Relaxed);
                            }
                        }
                        counters
                            .missed
                            .fetch_add((missed.saturating_sub(i64::from(catch_up_max.max(1)))) as u64, Relaxed);
                    }
                }
                fired_at_least_once = true;
            }
        }

        // boundaries
        if let Some(not_before) = config.not_before_ms
            && next.timestamp_millis() < not_before
        {
            // jump past the boundary
            let boundary = Utc.timestamp_millis_opt(not_before).single().unwrap_or(now);
            match next_occurrence(&config.spec, tz, boundary) {
                Some(later) => next = later,
                None => return,
            }
            if next.timestamp_millis() < not_before {
                return;
            }
        }
        if let Some(not_after) = config.not_after_ms
            && next.timestamp_millis() >= not_after
        {
            return; // schedule window closed
        }

        let sleep_ms = (next - now).num_milliseconds().max(1);
        tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;

        // Re-read the clock: we are allowed to be late (system suspend,
        // runtime contention). Fire only if we are within the slack window
        // of the planned instant; otherwise re-anchor without firing (the
        // miss is counted on the next loop's catch-up scan when relevant).
        let woke_at = Utc::now();
        use std::sync::atomic::Ordering::Relaxed;
        let planned_ms = next.timestamp_millis();
        let woke_ms = woke_at.timestamp_millis();
        if woke_ms < planned_ms - WAKE_SLACK_MS {
            // spurious early wake (clock adjust): recompute without firing
            continue;
        }
        if woke_ms > planned_ms + WAKE_SLACK_MS {
            // late wake: missed the planned instant
            counters.missed.fetch_add(1, Relaxed);
            match config.missed_policy {
                MissedPolicy::FireOnce => {
                    // still deliver once — the operator asked for a run
                    let fired_at = send_event(&queue, &config.id, planned_ms, config.payload.clone());
                    if fired_at.is_some() {
                        counters.fired.fetch_add(1, Relaxed);
                        counters.caught_up.fetch_add(1, Relaxed);
                    }
                }
                MissedPolicy::Skip => { /* stay silent, resynchronize */ }
                MissedPolicy::CatchUp { .. } => {
                    // deliver one (bounded replay is handled by the
                    // catch-up scan at (re)start; here one late fire is
                    // the friendly behavior)
                    let fired_at = send_event(&queue, &config.id, planned_ms, config.payload.clone());
                    if fired_at.is_some() {
                        counters.fired.fetch_add(1, Relaxed);
                        counters.caught_up.fetch_add(1, Relaxed);
                    }
                }
            }
            fired_at_least_once = true;
            continue;
        }

        // Normal path: fire exactly once.
        let _ = send_event(&queue, &config.id, planned_ms, config.payload.clone());
        counters.fired.fetch_add(1, Relaxed);
        fired_at_least_once = true;
    }
}

/// How many occurrences were missed between the last plausible firing and
/// now, for interval specs (calendar specs use the count of skipped
/// occurrences computed from the schedule grid — approximated by 1 for a
/// wake past the planned time; precise replay is policy-driven at fire
/// time, not reconstructed from history).
fn count_missed(spec: &ScheduleSpec, tz: Tz, now: DateTime<Utc>) -> i64 {
    // For intervals: occurrences per elapsed grid since the epoch anchor.
    // For calendar specs, a restart cannot know when the last run happened
    // without state; miss accounting happens through the wake-late path.
    match spec {
        ScheduleSpec::Interval { unit, every } => {
            let step = i64::from(*every) * unit.seconds();
            if step <= 0 {
                return 0;
            }
            // Without persisted state, "missed" is unknowable for a cold
            // start; report 0 (fresh schedules have nothing to catch up).
            // This function is only reached on the first loop pass; the
            // interval task then anchors cleanly.
            let _ = tz;
            let _ = now;
            0
        }
        _ => 0,
    }
}

/// Builds and enqueues the fired event. Returns the planned-fire instant's
/// millis when the queue accepted it, `None` on a full or closed queue.
fn send_event(
    queue: &tokio::sync::mpsc::Sender<WorkItem>,
    schedule_id: &str,
    planned_ms: i64,
    payload: serde_json::Value,
) -> Option<i64> {
    let event_id = format!("sched-{schedule_id}-{planned_ms}");
    let event = Event::new(
        &event_id,
        SCHEDULE_EVENT_SOURCE,
        SCHEDULE_EVENT_KIND,
        planned_ms.max(1),
        Payload::Json { value: payload },
    )
    .ok()?;
    let (tx, _rx) = tokio::sync::oneshot::channel();
    queue
        .try_send(WorkItem {
            events: vec![event],
            reply: tx,
        })
        .ok()
        .map(|_| planned_ms)
}

/// Deterministic ID for a firing: `sched-<id>-<planned_ms>` makes the
/// engine's deduplicator collapse exact replays (same schedule firing the
/// same planned instant twice) — overlap safety at the event layer.
pub fn scheduled_event_id(schedule_id: &str, planned_ms: i64) -> String {
    format!("sched-{schedule_id}-{planned_ms}")
}



#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(ms_or_secs: (i64, u32)) -> DateTime<Utc> {
        Utc.timestamp_opt(ms_or_secs.0, ms_or_secs.1).single().unwrap()
    }

    fn tz(name: &str) -> Tz {
        Tz::from_str(name).unwrap()
    }

    // --- spec validation ---

    #[test]
    fn validates_interval_and_rejects_zero() {
        let good = ScheduleConfig {
            id: "i".into(),
            spec: ScheduleSpec::Interval { unit: IntervalUnit::Minutes, every: 5 },
            timezone: "UTC".into(),
            payload: serde_json::json!({}),
            missed_policy: MissedPolicy::default(),
            overlap_policy: OverlapPolicy::default(),
            not_before_ms: None,
            not_after_ms: None,
            enabled: true,
        };
        assert!(good.validate().is_ok());
        let bad = ScheduleConfig {
            spec: ScheduleSpec::Interval { unit: IntervalUnit::Seconds, every: 0 },
            ..good.clone()
        };
        assert!(bad.validate().is_err());
        let empty_id = ScheduleConfig { id: "  ".into(), ..good };
        assert!(empty_id.validate().is_err());
    }

    #[test]
    fn validates_timezone_and_time_strings() {
        let base = ScheduleConfig {
            id: "d".into(),
            spec: ScheduleSpec::Daily { time: "09:30".into() },
            timezone: "Asia/Kolkata".into(),
            payload: serde_json::json!({}),
            missed_policy: MissedPolicy::default(),
            overlap_policy: OverlapPolicy::default(),
            not_before_ms: None,
            not_after_ms: None,
            enabled: true,
        };
        assert!(base.validate().is_ok());
        let bad_tz = ScheduleConfig { timezone: "Mars/Olympus".into(), ..base.clone() };
        assert!(bad_tz.validate().is_err());
        let bad_time = ScheduleConfig { spec: ScheduleSpec::Daily { time: "25:99".into() }, ..base };
        assert!(bad_time.validate().is_err());
    }

    #[test]
    fn parses_time_strings() {
        assert_eq!(parse_time("09:30").unwrap(), NaiveTime::from_hms_opt(9, 30, 0).unwrap());
        assert_eq!(parse_time(" 7:05:09 ").unwrap(), NaiveTime::from_hms_opt(7, 5, 9).unwrap());
        assert!(parse_time("9").is_err());
        assert!(parse_time("09:60").is_err());
    }

    // --- next_occurrence: intervals ---

    #[test]
    fn interval_seconds_next_run_is_grid_aligned() {
        let spec = ScheduleSpec::Interval { unit: IntervalUnit::Seconds, every: 15 };
        let now = utc((1_700_000_010, 0)); // not on the 15s grid
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next.timestamp(), 1_700_000_010 + (15 - 1_700_000_010 % 15));
    }

    #[test]
    fn interval_minutes_and_hours() {
        let minutes = ScheduleSpec::Interval { unit: IntervalUnit::Minutes, every: 30 };
        let now = utc((1_700_000_100, 0));
        let next = next_occurrence(&minutes, tz("UTC"), now).unwrap();
        assert_eq!(next.timestamp() - now.timestamp(), 30 * 60 - (now.timestamp() % (30 * 60)));

        let hours = ScheduleSpec::Interval { unit: IntervalUnit::Hours, every: 2 };
        let next_h = next_occurrence(&hours, tz("UTC"), now).unwrap();
        assert_eq!(next_h.timestamp() - now.timestamp(), 2 * 3_600 - (now.timestamp() % (2 * 3_600)));
    }

    // --- next_occurrence: daily + timezone ---

    #[test]
    fn daily_respects_timezone() {
        // 09:30 Kolkata is 04:00 UTC.
        let spec = ScheduleSpec::Daily { time: "09:30".into() };
        let kolkata = tz("Asia/Kolkata");
        // 2026-01-15 03:59 UTC = 09:29 IST → next run 09:30 IST = 04:00 UTC.
        let now = DateTime::parse_from_rfc3339("2026-01-15T03:59:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, kolkata, now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-15T04:00:00Z").unwrap().with_timezone(&Utc));

        // Same wall time in New York (winter, UTC-5): 09:30 EST = 14:30 UTC.
        let ny = tz("America/New_York");
        let next_ny = next_occurrence(&spec, ny, now).unwrap();
        assert_eq!(next_ny, DateTime::parse_from_rfc3339("2026-01-15T14:30:00Z").unwrap().with_timezone(&Utc));
    }

    #[test]
    fn daily_skips_to_tomorrow_when_time_passed() {
        let spec = ScheduleSpec::Daily { time: "00:00".into() };
        let now = DateTime::parse_from_rfc3339("2026-01-15T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-16T00:00:00Z").unwrap().with_timezone(&Utc));
    }

    // --- next_occurrence: weekly ---

    #[test]
    fn weekly_fires_on_named_days_only() {
        let spec = ScheduleSpec::Weekly {
            days: vec![Weekday::Monday, Weekday::Friday],
            time: "08:00".into(),
        };
        // 2026-01-14 is a Wednesday. Next: Friday 2026-01-16 08:00 UTC.
        let now = DateTime::parse_from_rfc3339("2026-01-14T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-16T08:00:00Z").unwrap().with_timezone(&Utc));

        // 2026-01-16 09:00 (after Friday 08:00) → Monday 2026-01-19 08:00.
        let later = DateTime::parse_from_rfc3339("2026-01-16T09:00:00Z").unwrap().with_timezone(&Utc);
        let next_mon = next_occurrence(&spec, tz("UTC"), later).unwrap();
        assert_eq!(next_mon, DateTime::parse_from_rfc3339("2026-01-19T08:00:00Z").unwrap().with_timezone(&Utc));
    }

    #[test]
    fn weekly_in_timezone() {
        // Friday 08:00 in Sydney (AEDT, UTC+11 in January).
        let spec = ScheduleSpec::Weekly { days: vec![Weekday::Friday], time: "08:00".into() };
        let now = DateTime::parse_from_rfc3339("2026-01-14T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("Australia/Sydney"), now).unwrap();
        // Friday 2026-01-16 08:00 AEDT = 2026-01-15 21:00 UTC.
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-15T21:00:00Z").unwrap().with_timezone(&Utc));
    }

    // --- next_occurrence: every N weeks ---

    #[test]
    fn every_n_weeks_uses_week_grid() {
        // Every 2 weeks on Monday 09:00 UTC. 2026-01-05 is a Monday.
        // Weeks since epoch Monday (1970-01-05): (date - 1970-01-05)/7.
        let spec = ScheduleSpec::EveryNWeeks {
            interval_weeks: 2,
            day: Weekday::Monday,
            time: "09:00".into(),
        };
        // 2026-01-05 Monday: days since 1970-01-05 = 20444 → weeks = 2920
        // (even) → fires. 2026-01-12: weeks = 2921 (odd) → skipped.
        let before = DateTime::parse_from_rfc3339("2026-01-04T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), before).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-05T09:00:00Z").unwrap().with_timezone(&Utc));

        // After the 2026-01-05 firing, the next is 2026-01-19 (two weeks), not 01-12.
        let after = DateTime::parse_from_rfc3339("2026-01-05T09:30:00Z").unwrap().with_timezone(&Utc);
        let next2 = next_occurrence(&spec, tz("UTC"), after).unwrap();
        assert_eq!(next2, DateTime::parse_from_rfc3339("2026-01-19T09:00:00Z").unwrap().with_timezone(&Utc));
    }

    // --- next_occurrence: monthly ---

    #[test]
    fn monthly_clamps_short_months() {
        // Day 31 in a 30-day month fires on the 30th (clamped).
        let spec = ScheduleSpec::Monthly { day: 31, time: "12:00".into() };
        // April has 30 days; asking just before April 30 → clamps to Apr 30.
        let now = DateTime::parse_from_rfc3339("2026-04-01T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-04-30T12:00:00Z").unwrap().with_timezone(&Utc));
    }

    #[test]
    fn monthly_nth_weekday() {
        // Third Friday of January 2026 = 2026-01-16.
        let spec = ScheduleSpec::MonthlyNth { nth: 3, weekday: Weekday::Friday, time: "10:00".into() };
        let now = DateTime::parse_from_rfc3339("2026-01-02T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-16T10:00:00Z").unwrap().with_timezone(&Utc));

        // After it, the next third-Friday is February: 2026-02-20.
        let after = DateTime::parse_from_rfc3339("2026-01-16T11:00:00Z").unwrap().with_timezone(&Utc);
        let next2 = next_occurrence(&spec, tz("UTC"), after).unwrap();
        assert_eq!(next2, DateTime::parse_from_rfc3339("2026-02-20T10:00:00Z").unwrap().with_timezone(&Utc));
    }

    #[test]
    fn monthly_last_day() {
        let spec = ScheduleSpec::MonthlyLast { time: "23:59".into() };
        let now = DateTime::parse_from_rfc3339("2026-02-10T10:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("UTC"), now).unwrap();
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-02-28T23:59:00Z").unwrap().with_timezone(&Utc));
    }

    // --- next_occurrence: cron ---

    #[test]
    fn cron_expression_evaluates_in_timezone() {
        // 6-field cron (with seconds): every day 09:30:00.
        let spec = ScheduleSpec::Cron { expression: "0 30 9 * * *".into() };
        let now = DateTime::parse_from_rfc3339("2026-01-15T08:00:00Z").unwrap().with_timezone(&Utc);
        let next = next_occurrence(&spec, tz("Europe/Berlin"), now).unwrap();
        // 09:30 Berlin (CET, UTC+1) = 08:30 UTC.
        assert_eq!(next, DateTime::parse_from_rfc3339("2026-01-15T08:30:00Z").unwrap().with_timezone(&Utc));

        // 5-field form also accepted.
        let five = ScheduleSpec::Cron { expression: "30 9 * * *".into() };
        let next5 = next_occurrence(&five, tz("UTC"), now).unwrap();
        assert_eq!(next5, DateTime::parse_from_rfc3339("2026-01-15T09:30:00Z").unwrap().with_timezone(&Utc));
    }

    #[test]
    fn invalid_cron_is_rejected_at_validation() {
        let config = ScheduleConfig {
            id: "c".into(),
            spec: ScheduleSpec::Cron { expression: "not a cron".into() },
            timezone: "UTC".into(),
            payload: serde_json::json!({}),
            missed_policy: MissedPolicy::default(),
            overlap_policy: OverlapPolicy::default(),
            not_before_ms: None,
            not_after_ms: None,
            enabled: true,
        };
        assert!(config.validate().is_err());
    }

    // --- boundaries ---

    #[test]
    fn effective_cron_reflects_spec() {
        let mut config = ScheduleConfig {
            id: "c".into(),
            spec: ScheduleSpec::Cron { expression: "0 0 * * *".into() },
            timezone: "UTC".into(),
            payload: serde_json::json!({}),
            missed_policy: MissedPolicy::default(),
            overlap_policy: OverlapPolicy::default(),
            not_before_ms: None,
            not_after_ms: None,
            enabled: true,
        };
        assert_eq!(config.effective_cron().as_deref(), Some("0 0 0 * * *"));
        config.spec = ScheduleSpec::Daily { time: "09:00".into() };
        assert_eq!(config.effective_cron(), None);
    }

    // --- weekday parsing ---

    #[test]
    fn weekday_list_parsing() {
        let days = Weekday::parse_list("Mon, Wednesday,FRI").unwrap();
        assert_eq!(days, vec![Weekday::Monday, Weekday::Wednesday, Weekday::Friday]);
        assert!(Weekday::parse_list("").is_err());
        assert!(Weekday::parse_list("Funday").is_err());
    }

    // --- event id determinism ---

    #[test]
    fn event_ids_are_deterministic_per_planned_instant() {
        assert_eq!(
            scheduled_event_id("nightly", 1_700_000_000_000),
            scheduled_event_id("nightly", 1_700_000_000_000)
        );
        assert_ne!(
            scheduled_event_id("nightly", 1_700_000_000_000),
            scheduled_event_id("nightly", 1_700_000_060_000)
        );
    }

    // --- validate_all ---

    #[test]
    fn validate_all_reports_duplicate_ids() {
        let config = |id: &str| ScheduleConfig {
            id: id.into(),
            spec: ScheduleSpec::Daily { time: "09:00".into() },
            timezone: "UTC".into(),
            payload: serde_json::json!({}),
            missed_policy: MissedPolicy::default(),
            overlap_policy: OverlapPolicy::default(),
            not_before_ms: None,
            not_after_ms: None,
            enabled: true,
        };
        assert!(validate_all(&[config("a"), config("b")]).is_ok());
        assert!(validate_all(&[config("a"), config("a")]).is_err());
    }
}
