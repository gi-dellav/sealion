//! Host helpers for the `Date` namespace and date instances.
//!
//! A `Date` is a [`JsObject`] whose `date` field holds the time value: whole
//! milliseconds since the epoch, `NaN` when the date is invalid. The local
//! timezone comes from the system clock through `chrono`, so `getHours` and
//! friends answer real local time; the timezone-database lookups happen per
//! call, and nothing is cached, because a process may outlive a DST change.
//!
//! The methods reuse the native-closure pattern of `jit::strings`: the record's
//! `env` is the receiver, so `var f = d.getTime` works and `this` is ignored.
//!
//! The deviation from the specification is recorded as `TODO(error-handling)`:
//! the host ABI has no exception channel, so `toISOString` on an invalid date
//! returns the string `"Invalid Date"` instead of raising a `RangeError`.

use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Local, Offset, TimeZone, Timelike, Utc};
use sealion_string::JsString;

use super::arrays::{arg, arg_count};
use super::objects::{
    JsObject, define_property, native_function, new_date, new_object, tagged_object, with_object,
};
use super::strings::{
    is_object, is_string, tagged_null, tagged_string, tagged_undefined, to_js_string,
    to_number_value,
};
use crate::value::{PAYLOAD_MASK, TAG_OBJECT};

/// The largest magnitude a time value may have: 100 000 000 days.
const MAX_TIME: f64 = 8.64e15;

/// `TimeClip`: `ToIntegerOrInfinity`, then clamped to [`MAX_TIME`].
fn time_clip(time: f64) -> f64 {
    if !time.is_finite() || time.abs() > MAX_TIME {
        return f64::NAN;
    }
    time.trunc()
}

fn now_ms() -> f64 {
    Utc::now().timestamp_millis() as f64
}

fn utc_time(time: f64) -> Option<DateTime<Utc>> {
    if !time.is_finite() {
        return None;
    }
    DateTime::from_timestamp_millis(time as i64)
}

fn local_time(time: f64) -> Option<DateTime<Local>> {
    utc_time(time).map(|time| time.with_timezone(&Local))
}

/// The local timezone offset in milliseconds east of UTC at `time`.
fn offset_ms(time: f64) -> f64 {
    let Some(local) = local_time(time) else {
        return 0.0;
    };
    f64::from(local.offset().fix().local_minus_utc()) * 1000.0
}

/// `MakeTime` from a local wall clock: the zone offset is guessed once and then
/// corrected, which resolves the offsets around a DST transition.
fn from_local_wall_clock(wall: f64) -> f64 {
    let first = wall - offset_ms(wall);
    time_clip(wall - offset_ms(first))
}

/// Days since the epoch of a civil date, months and days allowed to overflow
/// (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The field values of one instant, read either in UTC or in local time.
#[derive(Clone, Copy)]
struct Fields {
    year: i32,
    month: u32,
    day: u32,
    weekday: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
}

impl Fields {
    fn of<Tz: TimeZone>(time: &DateTime<Tz>) -> Self {
        Self {
            year: time.year(),
            month: time.month0(),
            day: time.day(),
            weekday: time.weekday().num_days_from_sunday(),
            hour: time.hour(),
            minute: time.minute(),
            second: time.second(),
            millis: time.timestamp_subsec_millis(),
        }
    }
}

fn fields(time: f64, utc: bool) -> Option<Fields> {
    if utc {
        utc_time(time).map(|time| Fields::of(&time))
    } else {
        local_time(time).map(|time| Fields::of(&time))
    }
}

#[derive(Clone, Copy)]
enum Getter {
    Time,
    FullYear(bool),
    Month(bool),
    Date(bool),
    Day(bool),
    Hours(bool),
    Minutes(bool),
    Seconds(bool),
    Millis(bool),
    TimezoneOffset,
}

fn getter(key: &str) -> Option<Getter> {
    Some(match key {
        "getTime" | "valueOf" => Getter::Time,
        "getFullYear" => Getter::FullYear(false),
        "getMonth" => Getter::Month(false),
        "getDate" => Getter::Date(false),
        "getDay" => Getter::Day(false),
        "getHours" => Getter::Hours(false),
        "getMinutes" => Getter::Minutes(false),
        "getSeconds" => Getter::Seconds(false),
        "getMilliseconds" => Getter::Millis(false),
        "getUTCFullYear" => Getter::FullYear(true),
        "getUTCMonth" => Getter::Month(true),
        "getUTCDate" => Getter::Date(true),
        "getUTCDay" => Getter::Day(true),
        "getUTCHours" => Getter::Hours(true),
        "getUTCMinutes" => Getter::Minutes(true),
        "getUTCSeconds" => Getter::Seconds(true),
        "getUTCMilliseconds" => Getter::Millis(true),
        "getTimezoneOffset" => Getter::TimezoneOffset,
        _ => return None,
    })
}

fn read_getter(kind: Getter, time: f64) -> f64 {
    if !time.is_finite() {
        return f64::NAN;
    }
    if let Getter::Time = kind {
        return time;
    }
    if let Getter::TimezoneOffset = kind {
        // `getTimezoneOffset` is UTC minus local, in minutes, with the opposite
        // sign to the offset east of UTC.
        return -offset_ms(time) / 60_000.0;
    }
    let utc = matches!(
        kind,
        Getter::FullYear(true)
            | Getter::Month(true)
            | Getter::Date(true)
            | Getter::Day(true)
            | Getter::Hours(true)
            | Getter::Minutes(true)
            | Getter::Seconds(true)
            | Getter::Millis(true)
    );
    let Some(fields) = fields(time, utc) else {
        return f64::NAN;
    };
    match kind {
        Getter::FullYear(_) => f64::from(fields.year),
        Getter::Month(_) => f64::from(fields.month),
        Getter::Date(_) => f64::from(fields.day),
        Getter::Day(_) => f64::from(fields.weekday),
        Getter::Hours(_) => f64::from(fields.hour),
        Getter::Minutes(_) => f64::from(fields.minute),
        Getter::Seconds(_) => f64::from(fields.second),
        Getter::Millis(_) => f64::from(fields.millis),
        Getter::Time | Getter::TimezoneOffset => f64::NAN,
    }
}

const INVALID: &str = "Invalid Date";

/// `Date.prototype.toString`, or [`INVALID`] when the time value is `NaN`.
fn to_string_local(time: f64) -> String {
    match local_time(time) {
        Some(time) => time.format("%a %b %d %Y %H:%M:%S GMT%z").to_string(),
        None => INVALID.to_string(),
    }
}

fn to_iso_string(time: f64) -> String {
    match utc_time(time) {
        // TODO(error-handling): an invalid date is a `RangeError` in the spec.
        None => INVALID.to_string(),
        Some(time) => time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
    }
}

fn to_utc_string(time: f64) -> String {
    match utc_time(time) {
        Some(time) => time.format("%a, %d %b %Y %H:%M:%S GMT").to_string(),
        None => INVALID.to_string(),
    }
}

fn to_date_string(time: f64) -> String {
    match local_time(time) {
        Some(time) => time.format("%a %b %d %Y").to_string(),
        None => INVALID.to_string(),
    }
}

fn to_time_string(time: f64) -> String {
    match local_time(time) {
        Some(time) => time.format("%H:%M:%S GMT%z").to_string(),
        None => INVALID.to_string(),
    }
}

/// `Date.parse` for the ISO 8601 forms the engine accepts. A date without a
/// time is UTC, a date and time without an offset is local, as the spec
/// requires.
fn parse_text(text: &str) -> f64 {
    let text = text.trim();
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return time_clip(time.timestamp_millis() as f64);
    }
    if let Ok(time) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f") {
        return from_local_wall_clock(time.and_utc().timestamp_millis() as f64);
    }
    let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .or_else(|_| chrono::NaiveDate::parse_from_str(&format!("{text}-01"), "%Y-%m-%d"))
        .or_else(|_| {
            text.parse::<i32>()
                .ok()
                .and_then(|year| chrono::NaiveDate::from_ymd_opt(year, 1, 1))
                .ok_or(())
        });
    match date {
        // A date without a time is UTC, as the specification requires.
        Ok(date) => time_clip(
            date.and_hms_opt(0, 0, 0)
                .map_or(f64::NAN, |time| time.and_utc().timestamp_millis() as f64),
        ),
        Err(()) => f64::NAN,
    }
}

/// The wall clock the 2..=7 argument `Date` constructor form names, in
/// milliseconds, with the defaults the spec uses: day 1, every time field zero,
/// and years `0..=99` in the 1900s.
fn wall_clock(args: *const u8) -> Option<f64> {
    let count = arg_count(args).min(7);
    let mut fields = [0.0f64; 7];
    fields[2] = 1.0;
    for (index, field) in fields.iter_mut().enumerate().take(count) {
        *field = to_number_value(arg(args, index));
        // A field beyond the time range makes the whole date invalid, which
        // also keeps the day arithmetic below inside `i64`.
        if !field.is_finite() || field.abs() > MAX_TIME {
            return None;
        }
        *field = field.trunc();
    }
    let year = if (0.0..=99.0).contains(&fields[0]) {
        fields[0] + 1900.0
    } else {
        fields[0]
    };
    let months = year as i64 * 12 + fields[1] as i64;
    let month = months.rem_euclid(12) + 1;
    let days = days_from_civil(months.div_euclid(12), month, 1) + fields[2] as i64 - 1;
    Some(
        days as f64 * 86_400_000.0
            + fields[3] * 3_600_000.0
            + fields[4] * 60_000.0
            + fields[5] * 1000.0
            + fields[6],
    )
}

/// `new Date(year, month, ...)`: the wall clock read as local time.
fn make_time(args: *const u8) -> f64 {
    wall_clock(args).map_or(f64::NAN, from_local_wall_clock)
}

/// `Date.UTC(year, month, ...)`: the same fields read as UTC.
fn make_utc_time(args: *const u8) -> f64 {
    wall_clock(args).map_or(f64::NAN, time_clip)
}

/// The time value behind `value`, or `None` when it is not a date. The
/// prototype answers `NaN` so that `Date.prototype.getTime` exists.
fn time_of(value: f64) -> Option<f64> {
    if !is_object(value) {
        return None;
    }
    if let Some(time) = with_object(value, |object| object.date).flatten() {
        return Some(time);
    }
    let pointer = (value.to_bits() & PAYLOAD_MASK) as usize;
    (pointer == date_prototype() as usize).then_some(f64::NAN)
}

fn set_time(value: f64, time: f64) {
    if !is_object(value) {
        return;
    }
    let pointer = (value.to_bits() & PAYLOAD_MASK) as *mut JsObject;
    if unsafe { (*pointer).date.is_some() } {
        unsafe { (*pointer).date = Some(time) };
    }
}

/// Every method a `Date` answers, in the order a closure record names them.
static METHODS: &[&str] = &[
    "getTime",
    "valueOf",
    "getFullYear",
    "getMonth",
    "getDate",
    "getDay",
    "getHours",
    "getMinutes",
    "getSeconds",
    "getMilliseconds",
    "getUTCFullYear",
    "getUTCMonth",
    "getUTCDate",
    "getUTCDay",
    "getUTCHours",
    "getUTCMinutes",
    "getUTCSeconds",
    "getUTCMilliseconds",
    "getTimezoneOffset",
    "setTime",
    "toISOString",
    "toJSON",
    "toString",
    "toLocaleString",
    "toDateString",
    "toLocaleDateString",
    "toTimeString",
    "toLocaleTimeString",
    "toUTCString",
];

/// The receiver and the index into [`METHODS`] of one native closure, boxed
/// together because the closure record only carries a single `env` word.
type Binding = (u64, u64);

fn binding_of(env: i64) -> (&'static Binding, &'static str) {
    let binding = unsafe { &*(env as usize as *const Binding) };
    (binding, METHODS[binding.1 as usize])
}

extern "C" fn date_method(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let (binding, name) = binding_of(env);
    let value = f64::from_bits((TAG_OBJECT << 48) | binding.0);
    let time = time_of(value).unwrap_or(f64::NAN);
    if let Some(kind) = getter(name) {
        return read_getter(kind, time);
    }
    match name {
        "setTime" => {
            let next = time_clip(to_number_value(arg(args, 0)));
            set_time(value, next);
            next
        }
        "toISOString" => tagged_string(JsString::from(to_iso_string(time))),
        "toJSON" => {
            if time.is_finite() {
                tagged_string(JsString::from(to_iso_string(time)))
            } else {
                tagged_null()
            }
        }
        "toString" | "toLocaleString" => tagged_string(JsString::from(to_string_local(time))),
        "toDateString" | "toLocaleDateString" => {
            tagged_string(JsString::from(to_date_string(time)))
        }
        "toTimeString" | "toLocaleTimeString" => {
            tagged_string(JsString::from(to_time_string(time)))
        }
        "toUTCString" => tagged_string(JsString::from(to_utc_string(time))),
        _ => tagged_undefined(),
    }
}

/// The native closure for `date.method`, or `None` when the key names no
/// method. Answered only by a `Date` and its prototype.
pub(super) fn method_property(value: f64, key: &JsString) -> Option<f64> {
    let key = key.to_std_string_lossy();
    let index = METHODS.iter().position(|name| *name == key)?;
    time_of(value)?;
    let binding = Box::into_raw(Box::new((value.to_bits() & PAYLOAD_MASK, index as u64)));
    Some(native_function(date_method, binding as u64))
}

/// The `toString` form of a `Date`, for `String(date)` and `console.log`.
pub(super) fn date_to_js_string(value: f64) -> Option<JsString> {
    let time = with_object(value, |object| object.date).flatten()?;
    Some(JsString::from(to_string_local(time)))
}

pub(super) fn date_prototype() -> *mut JsObject {
    static PROTOTYPE: OnceLock<usize> = OnceLock::new();
    *PROTOTYPE.get_or_init(|| new_object() as usize) as *mut JsObject
}

extern "C" fn sealion_date_prototype() -> u64 {
    date_prototype() as u64
}

/// `new Date(...)`: an instance over the time value the arguments describe.
extern "C" fn sealion_date_construct(args: *const u8) -> f64 {
    let time = match arg_count(args) {
        0 => now_ms(),
        1 if is_string(arg(args, 0)) => {
            parse_text(&to_js_string(arg(args, 0)).to_std_string_lossy())
        }
        1 => time_clip(to_number_value(arg(args, 0))),
        _ => make_time(args),
    };
    tagged_object(new_date(time))
}

/// `Date()`: the current time as a string.
extern "C" fn sealion_date_call() -> f64 {
    tagged_string(JsString::from(to_string_local(now_ms())))
}

extern "C" fn sealion_date_now() -> f64 {
    now_ms()
}

extern "C" fn sealion_date_parse(value: f64) -> f64 {
    parse_text(&to_js_string(value).to_std_string_lossy())
}

extern "C" fn sealion_date_utc(args: *const u8) -> f64 {
    make_utc_time(args)
}

/// The `Date` namespace object: `now`, `parse`, `UTC` and `prototype`.
extern "C" fn sealion_date_object() -> u64 {
    static OBJECT: OnceLock<usize> = OnceLock::new();
    *OBJECT.get_or_init(|| {
        let pointer = new_object();
        define_property(pointer, "now", native_function(date_now_method, 0));
        define_property(pointer, "parse", native_function(date_parse_method, 0));
        define_property(pointer, "UTC", native_function(date_utc_method, 0));
        define_property(pointer, "prototype", tagged_object(date_prototype()));
        pointer as usize
    }) as u64
}

extern "C" fn date_now_method(_env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    now_ms()
}

extern "C" fn date_parse_method(_env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    parse_text(&to_js_string(arg(args, 0)).to_std_string_lossy())
}

extern "C" fn date_utc_method(_env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    make_utc_time(args)
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_date_object" => sealion_date_object as *const () as usize,
        "sealion_date_prototype" => sealion_date_prototype as *const () as usize,
        "sealion_date_construct" => sealion_date_construct as *const () as usize,
        "sealion_date_call" => sealion_date_call as *const () as usize,
        "sealion_date_now" => sealion_date_now as *const () as usize,
        "sealion_date_parse" => sealion_date_parse as *const () as usize,
        "sealion_date_utc" => sealion_date_utc as *const () as usize,
        _ => return None,
    })
}
