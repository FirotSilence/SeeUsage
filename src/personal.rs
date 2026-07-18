use chinese_lunisolar_calendar::LunisolarDate;
use chrono::{Datelike, Days, Local, NaiveDate, NaiveDateTime, TimeZone, Weekday};
use serde::{Deserialize, Serialize};

use crate::i18n::{self, Language, key};

const MAX_NOTES: usize = 24;
const MAX_REMINDERS: usize = 32;
const MAX_NOTE_CHARS: usize = 80;
const MAX_REMINDER_CHARS: usize = 60;
const DISTANT_REMINDER_SECONDS: i64 = 2 * 24 * 60 * 60;
const MIN_LUNAR_YEAR: i32 = 1901;
const MAX_LUNAR_YEAR: i32 = 2100;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PersonalItems {
    pub notes: Vec<QuickNote>,
    pub reminders: Vec<Reminder>,
    next_id: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QuickNote {
    pub id: u64,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reminder {
    pub id: u64,
    pub text: String,
    pub next_at: i64,
    pub recurrence: Recurrence,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recurrence {
    Once,
    Interval {
        seconds: u64,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
    Weekdays {
        hour: u8,
        minute: u8,
    },
    Weekly {
        weekday: u8,
        hour: u8,
        minute: u8,
    },
    DaysOfWeek {
        weekdays: Vec<u8>,
        hour: u8,
        minute: u8,
    },
    Weekends {
        hour: u8,
        minute: u8,
    },
    Monthly {
        day: u8,
        hour: u8,
        minute: u8,
    },
    MonthlyLast {
        hour: u8,
        minute: u8,
    },
    Yearly {
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
    },
    LunarYearly {
        month: u8,
        day: u8,
        leap: bool,
        hour: u8,
        minute: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReminderPreset {
    AfterMinutes,
    AfterHours,
    AfterDays,
    AfterWeeks,
    EveryMinutes,
    EveryHours,
    EveryDays,
    EveryWeeks,
    Once,
    Daily,
    Weekdays,
    Weekly,
    SelectedWeekdays,
    Weekends,
    Monthly,
    MonthlyLast,
    Yearly,
    LunarYearly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudItemKind {
    Note,
    Reminder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HudItem {
    pub text: String,
    pub kind: HudItemKind,
    pub urgent: bool,
    pub due: bool,
    pub flashing: bool,
    pub reminder_id: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HudContent {
    pub items: Vec<HudItem>,
    pub clock: String,
}

impl PersonalItems {
    pub fn reminder_limit_reached(&self) -> bool {
        self.reminders.len() >= MAX_REMINDERS
    }

    pub fn add_note(&mut self, input: &str, language: Language) -> Result<(), String> {
        if self.notes.len() >= MAX_NOTES {
            return Err(i18n::template(
                language,
                key::MAX_NOTES,
                &[("max", MAX_NOTES.to_string())],
            ));
        }
        let text = clean_text(input, MAX_NOTE_CHARS, language)?;
        let id = self.take_id();
        self.notes.push(QuickNote { id, text });
        Ok(())
    }

    pub fn remove_note(&mut self, id: u64) -> bool {
        remove_by_id(&mut self.notes, id, |note| note.id)
    }

    pub fn add_reminder(
        &mut self,
        preset: ReminderPreset,
        label: &str,
        schedule: &str,
        now: i64,
        language: Language,
    ) -> Result<(), String> {
        if self.reminder_limit_reached() {
            return Err(i18n::template(
                language,
                key::MAX_REMINDERS,
                &[("max", MAX_REMINDERS.to_string())],
            ));
        }
        let text = clean_text(label, MAX_REMINDER_CHARS, language)?;
        let (next_at, recurrence) = parse_schedule(preset, schedule, now, language)?;
        let id = self.take_id();
        self.reminders.push(Reminder {
            id,
            text,
            next_at,
            recurrence,
        });
        Ok(())
    }

    pub fn remove_reminder(&mut self, id: u64) -> bool {
        remove_by_id(&mut self.reminders, id, |reminder| reminder.id)
    }

    pub fn acknowledge_due(&mut self, id: u64, now: i64) -> bool {
        let Some(index) = self
            .reminders
            .iter()
            .position(|reminder| reminder.id == id && reminder.next_at <= now)
        else {
            return false;
        };
        self.complete_at(index, now)
    }

    pub fn complete_next(&mut self, id: u64, now: i64) -> bool {
        let Some(index) = self.reminders.iter().position(|reminder| reminder.id == id) else {
            return false;
        };
        self.complete_at(index, now)
    }

    pub fn hud_content(
        &self,
        now: i64,
        show_distant_reminders: bool,
        language: Language,
    ) -> HudContent {
        let clock = local_time(now).format("%H:%M").to_string();
        let mut reminders = self
            .reminders
            .iter()
            .filter(|reminder| {
                show_distant_reminders
                    || reminder.next_at.saturating_sub(now) <= DISTANT_REMINDER_SECONDS
            })
            .collect::<Vec<_>>();
        reminders.sort_by_key(|reminder| {
            let remaining = reminder.next_at.saturating_sub(now);
            let urgency = if remaining <= 0 {
                0
            } else if remaining <= 5 * 60 {
                1
            } else {
                2
            };
            (urgency, reminder.next_at)
        });

        let mut items = Vec::with_capacity(self.reminders.len() + self.notes.len());
        items.extend(self.notes.iter().map(|note| HudItem {
            text: note.text.clone(),
            kind: HudItemKind::Note,
            urgent: false,
            due: false,
            flashing: false,
            reminder_id: None,
        }));
        items.extend(reminders.into_iter().map(|reminder| HudItem {
            text: reminder_hud_text(reminder, now, language),
            kind: HudItemKind::Reminder,
            urgent: reminder.next_at.saturating_sub(now) <= 5 * 60,
            due: reminder.next_at <= now,
            flashing: reminder.next_at <= now && now.rem_euclid(2) == 0,
            reminder_id: Some(reminder.id),
        }));

        HudContent { items, clock }
    }

    pub fn reminder_menu_text(&self, reminder: &Reminder, language: Language) -> String {
        let schedule = match reminder.recurrence {
            Recurrence::Once => local_time(reminder.next_at)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            _ => reminder.recurrence.label(language),
        };
        format!("{schedule} - {}", reminder.text)
    }

    fn take_id(&mut self) -> u64 {
        let minimum = self
            .notes
            .iter()
            .map(|note| note.id)
            .chain(self.reminders.iter().map(|reminder| reminder.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_id = self.next_id.max(minimum).max(1);
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1).max(1);
        id
    }

    fn complete_at(&mut self, index: usize, now: i64) -> bool {
        let reminder = &self.reminders[index];
        let due = reminder.next_at;
        let next_at = reminder.recurrence.next_after_due(now.max(due), due);
        if let Some(next_at) = next_at {
            self.reminders[index].next_at = next_at;
        } else {
            self.reminders.remove(index);
        }
        true
    }
}

impl Recurrence {
    fn next_after(&self, after: i64) -> Option<i64> {
        match self {
            Self::Once => None,
            Self::Interval { seconds } => add_interval(after, *seconds),
            Self::Daily { hour, minute } => next_matching(after, *hour, *minute, |_| true, 2),
            Self::Weekdays { hour, minute } => next_matching(
                after,
                *hour,
                *minute,
                |date| !matches!(date.weekday(), Weekday::Sat | Weekday::Sun),
                8,
            ),
            Self::Weekly {
                weekday,
                hour,
                minute,
            } => next_matching(
                after,
                *hour,
                *minute,
                |date| date.weekday().num_days_from_monday() as u8 == *weekday,
                8,
            ),
            Self::DaysOfWeek {
                weekdays,
                hour,
                minute,
            } => next_matching(
                after,
                *hour,
                *minute,
                |date| weekdays.contains(&(date.weekday().num_days_from_monday() as u8)),
                8,
            ),
            Self::Weekends { hour, minute } => next_matching(
                after,
                *hour,
                *minute,
                |date| matches!(date.weekday(), Weekday::Sat | Weekday::Sun),
                8,
            ),
            Self::Monthly { day, hour, minute } => {
                next_matching(after, *hour, *minute, |date| date.day() == *day as u32, 370)
            }
            Self::MonthlyLast { hour, minute } => next_matching(
                after,
                *hour,
                *minute,
                |date| {
                    date.checked_add_days(Days::new(1))
                        .is_some_and(|tomorrow| tomorrow.month() != date.month())
                },
                40,
            ),
            Self::Yearly {
                month,
                day,
                hour,
                minute,
            } => next_matching(
                after,
                *hour,
                *minute,
                |date| date.month() == *month as u32 && date.day() == *day as u32,
                366 * 5,
            ),
            Self::LunarYearly {
                month,
                day,
                leap,
                hour,
                minute,
            } => next_lunar_yearly(after, *month, *day, *leap, *hour, *minute),
        }
    }

    fn next_after_due(&self, after: i64, due: i64) -> Option<i64> {
        match self {
            Self::Interval { seconds } => next_interval_after(after, due, *seconds),
            _ => self.next_after(after),
        }
    }

    fn label(&self, language: Language) -> String {
        match self {
            Self::Once => i18n::text(language, key::ONE_TIME).into(),
            Self::Interval { seconds } => interval_label(*seconds, language),
            Self::Daily { hour, minute } => {
                format!("{} {hour:02}:{minute:02}", i18n::text(language, key::DAILY))
            }
            Self::Weekdays { hour, minute } => format!(
                "{} {hour:02}:{minute:02}",
                i18n::text(language, key::WEEKDAYS)
            ),
            Self::Weekly {
                weekday,
                hour,
                minute,
            } => format!(
                "{} {hour:02}:{minute:02}",
                weekday_name(*weekday, language)
                    .unwrap_or_else(|| i18n::text(language, key::WEEKLY))
            ),
            Self::DaysOfWeek {
                weekdays,
                hour,
                minute,
            } => format!(
                "{} {hour:02}:{minute:02}",
                weekday_list_label(weekdays, language)
            ),
            Self::Weekends { hour, minute } => format!(
                "{} {hour:02}:{minute:02}",
                i18n::text(language, key::WEEKENDS)
            ),
            Self::Monthly { day, hour, minute } => {
                format!(
                    "{} {day} {hour:02}:{minute:02}",
                    i18n::text(language, key::MONTH_DAY)
                )
            }
            Self::MonthlyLast { hour, minute } => {
                format!(
                    "{} {hour:02}:{minute:02}",
                    i18n::text(language, key::LAST_DAY_MONTHLY)
                )
            }
            Self::Yearly {
                month,
                day,
                hour,
                minute,
            } => format!(
                "{} {} {day} {hour:02}:{minute:02}",
                i18n::text(language, key::SOLAR_YEARLY),
                month_name(*month, language).unwrap_or_else(|| month.to_string())
            ),
            Self::LunarYearly {
                month,
                day,
                leap,
                hour,
                minute,
            } => format!(
                "{} {}{month}/{day} {hour:02}:{minute:02}",
                i18n::text(language, key::LUNAR),
                if *leap {
                    format!("{} ", i18n::text(language, key::LEAP))
                } else {
                    String::new()
                }
            ),
        }
    }
}

pub fn preset_prompt(preset: ReminderPreset, now: i64, language: Language) -> (String, String) {
    match preset {
        ReminderPreset::AfterMinutes | ReminderPreset::EveryMinutes => (
            format!("{} (1-10080):", i18n::text(language, key::INPUT_VALUE)),
            "25".into(),
        ),
        ReminderPreset::AfterHours | ReminderPreset::EveryHours => (
            format!("{} (1-8760):", i18n::text(language, key::INPUT_VALUE)),
            "1".into(),
        ),
        ReminderPreset::AfterDays | ReminderPreset::EveryDays => (
            format!("{} (1-365):", i18n::text(language, key::INPUT_VALUE)),
            "1".into(),
        ),
        ReminderPreset::AfterWeeks | ReminderPreset::EveryWeeks => (
            format!("{} (1-52):", i18n::text(language, key::INPUT_VALUE)),
            "1".into(),
        ),
        ReminderPreset::Once => (
            i18n::text(language, key::DATE_TIME_FORMAT).into(),
            local_time(now.saturating_add(60 * 60))
                .format("%Y-%m-%d %H:%M")
                .to_string(),
        ),
        ReminderPreset::Daily | ReminderPreset::Weekdays | ReminderPreset::Weekends => (
            i18n::text(language, key::TIME_FORMAT).into(),
            "09:00".into(),
        ),
        ReminderPreset::Weekly => (
            i18n::text(language, key::WEEKDAY_TIME_FORMAT).into(),
            "Mon 09:00".into(),
        ),
        ReminderPreset::SelectedWeekdays => (
            i18n::text(language, key::WEEKDAYS_TIME_FORMAT).into(),
            "Mon,Wed,Fri 09:00".into(),
        ),
        ReminderPreset::Monthly => (
            i18n::text(language, key::DAY_TIME_FORMAT).into(),
            "1 09:00".into(),
        ),
        ReminderPreset::MonthlyLast => (
            i18n::text(language, key::TIME_FORMAT).into(),
            "09:00".into(),
        ),
        ReminderPreset::Yearly => (
            i18n::text(language, key::SOLAR_TIME_FORMAT).into(),
            "Jan 1 09:00".into(),
        ),
        ReminderPreset::LunarYearly => (
            i18n::text(language, key::LUNAR_TIME_FORMAT).into(),
            "1 1 09:00".into(),
        ),
    }
}

pub fn preset_label(preset: ReminderPreset, language: Language) -> String {
    match preset {
        ReminderPreset::AfterMinutes => preset_group(language, key::AFTER_DELAY, key::MINUTES),
        ReminderPreset::AfterHours => preset_group(language, key::AFTER_DELAY, key::HOURS),
        ReminderPreset::AfterDays => preset_group(language, key::AFTER_DELAY, key::DAYS),
        ReminderPreset::AfterWeeks => preset_group(language, key::AFTER_DELAY, key::WEEKS),
        ReminderPreset::EveryMinutes => preset_group(language, key::REPEAT_EVERY, key::MINUTES),
        ReminderPreset::EveryHours => preset_group(language, key::REPEAT_EVERY, key::HOURS),
        ReminderPreset::EveryDays => preset_group(language, key::REPEAT_EVERY, key::DAYS),
        ReminderPreset::EveryWeeks => preset_group(language, key::REPEAT_EVERY, key::WEEKS),
        ReminderPreset::Once => i18n::text(language, key::ONE_TIME).into(),
        ReminderPreset::Daily => i18n::text(language, key::DAILY).into(),
        ReminderPreset::Weekdays => i18n::text(language, key::WEEKDAYS).into(),
        ReminderPreset::Weekly => i18n::text(language, key::WEEKLY).into(),
        ReminderPreset::SelectedWeekdays => i18n::text(language, key::SELECTED_WEEKDAYS).into(),
        ReminderPreset::Weekends => i18n::text(language, key::WEEKENDS).into(),
        ReminderPreset::Monthly => i18n::text(language, key::MONTHLY).into(),
        ReminderPreset::MonthlyLast => i18n::text(language, key::MONTHLY_LAST).into(),
        ReminderPreset::Yearly => i18n::text(language, key::SOLAR_DATE).into(),
        ReminderPreset::LunarYearly => i18n::text(language, key::LUNAR_DATE).into(),
    }
}

fn preset_group(language: Language, group: &'static str, unit: &'static str) -> String {
    format!(
        "{}: {}",
        i18n::text(language, group),
        i18n::text(language, unit)
    )
}

fn parse_schedule(
    preset: ReminderPreset,
    input: &str,
    now: i64,
    language: Language,
) -> Result<(i64, Recurrence), String> {
    match preset {
        ReminderPreset::AfterMinutes => {
            parse_relative(input, now, 60, 10_080, key::UNIT_MINUTES, false, language)
        }
        ReminderPreset::AfterHours => {
            parse_relative(input, now, 3_600, 8_760, key::UNIT_HOURS, false, language)
        }
        ReminderPreset::AfterDays => {
            parse_relative(input, now, 86_400, 365, key::UNIT_DAYS, false, language)
        }
        ReminderPreset::AfterWeeks => {
            parse_relative(input, now, 7 * 86_400, 52, key::UNIT_WEEKS, false, language)
        }
        ReminderPreset::EveryMinutes => {
            parse_relative(input, now, 60, 10_080, key::UNIT_MINUTES, true, language)
        }
        ReminderPreset::EveryHours => {
            parse_relative(input, now, 3_600, 8_760, key::UNIT_HOURS, true, language)
        }
        ReminderPreset::EveryDays => {
            parse_relative(input, now, 86_400, 365, key::UNIT_DAYS, true, language)
        }
        ReminderPreset::EveryWeeks => {
            parse_relative(input, now, 7 * 86_400, 52, key::UNIT_WEEKS, true, language)
        }
        ReminderPreset::Once => {
            let naive = NaiveDateTime::parse_from_str(input.trim(), "%Y-%m-%d %H:%M")
                .map_err(|_| i18n::text(language, key::DATE_TIME_FORMAT).to_string())?;
            let timestamp = Local
                .from_local_datetime(&naive)
                .earliest()
                .ok_or_else(|| i18n::text(language, key::LOCAL_TIME_MISSING).to_string())?
                .timestamp();
            if timestamp <= now {
                return Err(i18n::text(language, key::FUTURE_TIME_REQUIRED).into());
            }
            Ok((timestamp, Recurrence::Once))
        }
        ReminderPreset::Daily => {
            let (hour, minute) = parse_time(input, language)?;
            let recurrence = Recurrence::Daily { hour, minute };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::Weekdays => {
            let (hour, minute) = parse_time(input, language)?;
            let recurrence = Recurrence::Weekdays { hour, minute };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::Weekly => {
            let mut parts = input.split_whitespace();
            let weekday = parts
                .next()
                .and_then(parse_weekday)
                .ok_or_else(|| i18n::text(language, key::WEEKDAY_TIME_FORMAT).to_string())?;
            let time = parts
                .next()
                .ok_or_else(|| i18n::text(language, key::TIME_FORMAT).to_string())?;
            if parts.next().is_some() {
                return Err(i18n::text(language, key::WEEKDAY_TIME_FORMAT).into());
            }
            let (hour, minute) = parse_time(time, language)?;
            let recurrence = Recurrence::Weekly {
                weekday,
                hour,
                minute,
            };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::SelectedWeekdays => {
            let (weekdays, hour, minute) = parse_weekdays_and_time(input, language)?;
            let recurrence = Recurrence::DaysOfWeek {
                weekdays,
                hour,
                minute,
            };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::Weekends => {
            let (hour, minute) = parse_time(input, language)?;
            let recurrence = Recurrence::Weekends { hour, minute };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::Monthly => {
            let (day, hour, minute) = parse_monthly(input, language)?;
            let recurrence = Recurrence::Monthly { day, hour, minute };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::MonthlyLast => {
            let (hour, minute) = parse_time(input, language)?;
            let recurrence = Recurrence::MonthlyLast { hour, minute };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::Yearly => {
            let (month, day, hour, minute) = parse_yearly(input, language)?;
            let recurrence = Recurrence::Yearly {
                month,
                day,
                hour,
                minute,
            };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::NEXT_REMINDER_FAILED).to_string())?;
            Ok((next_at, recurrence))
        }
        ReminderPreset::LunarYearly => {
            let (month, day, leap, hour, minute) = parse_lunar_yearly(input, language)?;
            let recurrence = Recurrence::LunarYearly {
                month,
                day,
                leap,
                hour,
                minute,
            };
            let next_at = recurrence
                .next_after(now)
                .ok_or_else(|| i18n::text(language, key::LUNAR_NOT_FOUND).to_string())?;
            Ok((next_at, recurrence))
        }
    }
}

fn parse_relative(
    input: &str,
    now: i64,
    unit_seconds: u64,
    maximum: u64,
    unit_key: &'static str,
    repeats: bool,
    language: Language,
) -> Result<(i64, Recurrence), String> {
    let unit = i18n::text(language, unit_key).to_string();
    let count = input
        .trim()
        .parse::<u64>()
        .map_err(|_| i18n::template(language, key::WHOLE_NUMBER, &[("unit", unit.clone())]))?;
    if !(1..=maximum).contains(&count) {
        return Err(i18n::template(
            language,
            key::NUMBER_RANGE,
            &[("unit", unit), ("max", maximum.to_string())],
        ));
    }
    let seconds = count
        .checked_mul(unit_seconds)
        .ok_or_else(|| i18n::text(language, key::INTERVAL_TOO_LARGE).to_string())?;
    let next_at = add_interval(now, seconds)
        .ok_or_else(|| i18n::text(language, key::REMINDER_OUT_OF_RANGE).to_string())?;
    let recurrence = if repeats {
        Recurrence::Interval { seconds }
    } else {
        Recurrence::Once
    };
    Ok((next_at, recurrence))
}

fn parse_time(input: &str, language: Language) -> Result<(u8, u8), String> {
    let mut parts = input.trim().split(':');
    let hour = parts.next().and_then(|value| value.parse::<u8>().ok());
    let minute = parts.next().and_then(|value| value.parse::<u8>().ok());
    if parts.next().is_some() || hour.is_none() || minute.is_none() {
        return Err(i18n::text(language, key::TIME_FORMAT).into());
    }
    let (hour, minute) = (hour.unwrap(), minute.unwrap());
    if hour > 23 || minute > 59 {
        return Err(i18n::text(language, key::TIME_INVALID).into());
    }
    Ok((hour, minute))
}

fn parse_weekday(input: &str) -> Option<u8> {
    match input.trim().to_lowercase().as_str() {
        "mon" | "monday" | "周一" | "週一" | "星期一" | "月" | "月曜" | "月曜日" | "월"
        | "월요일" | "lun" | "lunes" | "lundi" | "mo" | "montag" => Some(0),
        "tue" | "tues" | "tuesday" | "周二" | "週二" | "星期二" | "火" | "火曜" | "火曜日"
        | "화" | "화요일" | "mar" | "martes" | "mardi" | "di" | "dienstag" => Some(1),
        "wed" | "wednesday" | "周三" | "週三" | "星期三" | "水" | "水曜" | "水曜日" | "수"
        | "수요일" | "mié" | "mie" | "miércoles" | "miercoles" | "mer" | "mercredi" | "mi"
        | "mittwoch" => Some(2),
        "thu" | "thur" | "thurs" | "thursday" | "周四" | "週四" | "星期四" | "木" | "木曜"
        | "木曜日" | "목" | "목요일" | "jue" | "jueves" | "jeu" | "jeudi" | "do" | "donnerstag" => {
            Some(3)
        }
        "fri" | "friday" | "周五" | "週五" | "星期五" | "金" | "金曜" | "金曜日" | "금"
        | "금요일" | "vie" | "viernes" | "ven" | "vendredi" | "fr" | "freitag" => Some(4),
        "sat" | "saturday" | "周六" | "週六" | "星期六" | "土" | "土曜" | "土曜日" | "토"
        | "토요일" | "sáb" | "sab" | "sábado" | "sabado" | "sam" | "samedi" | "sa" | "samstag" => {
            Some(5)
        }
        "sun" | "sunday" | "周日" | "週日" | "周天" | "星期日" | "星期天" | "日" | "日曜"
        | "日曜日" | "일" | "일요일" | "dom" | "domingo" | "dim" | "dimanche" | "so"
        | "sonntag" => Some(6),
        _ => None,
    }
}

fn parse_weekdays_and_time(input: &str, language: Language) -> Result<(Vec<u8>, u8, u8), String> {
    let mut parts = input.split_whitespace().collect::<Vec<_>>();
    let time = parts
        .pop()
        .ok_or_else(|| i18n::text(language, key::WEEKDAYS_TIME_FORMAT).to_string())?;
    if parts.is_empty() {
        return Err(i18n::text(language, key::WEEKDAY_REQUIRED).into());
    }
    let (hour, minute) = parse_time(time, language)?;
    let expression = parts.join(" ");
    let mut weekdays = Vec::new();
    for token in expression
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | ';' | '/'))
    {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_weekday(start).ok_or_else(|| {
                i18n::template(
                    language,
                    key::UNKNOWN_WEEKDAY,
                    &[("value", start.to_string())],
                )
            })?;
            let end = parse_weekday(end).ok_or_else(|| {
                i18n::template(
                    language,
                    key::UNKNOWN_WEEKDAY,
                    &[("value", end.to_string())],
                )
            })?;
            let mut day = start;
            for _ in 0..7 {
                weekdays.push(day);
                if day == end {
                    break;
                }
                day = (day + 1) % 7;
            }
        } else {
            weekdays.push(parse_weekday(token).ok_or_else(|| {
                i18n::template(
                    language,
                    key::UNKNOWN_WEEKDAY,
                    &[("value", token.to_string())],
                )
            })?);
        }
    }
    weekdays.sort_unstable();
    weekdays.dedup();
    if weekdays.is_empty() {
        return Err(i18n::text(language, key::WEEKDAY_REQUIRED).into());
    }
    Ok((weekdays, hour, minute))
}

fn parse_monthly(input: &str, language: Language) -> Result<(u8, u8, u8), String> {
    let parts = input.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(i18n::text(language, key::DAY_TIME_FORMAT).into());
    }
    let day = parts[0]
        .parse::<u8>()
        .ok()
        .filter(|day| (1..=31).contains(day))
        .ok_or_else(|| i18n::text(language, key::MONTH_DAY_RANGE).to_string())?;
    let (hour, minute) = parse_time(parts[1], language)?;
    Ok((day, hour, minute))
}

fn parse_yearly(input: &str, language: Language) -> Result<(u8, u8, u8, u8), String> {
    let parts = input.split_whitespace().collect::<Vec<_>>();
    let (month_text, day_text, time_text) = match parts.as_slice() {
        [month, day, time] => (*month, *day, *time),
        [date, time] => {
            let (month, day) = date
                .split_once(['-', '/'])
                .ok_or_else(|| i18n::text(language, key::SOLAR_TIME_FORMAT).to_string())?;
            (month, day, *time)
        }
        _ => return Err(i18n::text(language, key::SOLAR_TIME_FORMAT).into()),
    };
    let month = parse_month(month_text)
        .ok_or_else(|| i18n::text(language, key::MONTH_RANGE).to_string())?;
    let day = day_text
        .parse::<u8>()
        .ok()
        .filter(|day| (1..=31).contains(day))
        .ok_or_else(|| i18n::text(language, key::DAY_INVALID).to_string())?;
    if NaiveDate::from_ymd_opt(2000, month as u32, day as u32).is_none() {
        return Err(i18n::text(language, key::DATE_INVALID).into());
    }
    let (hour, minute) = parse_time(time_text, language)?;
    Ok((month, day, hour, minute))
}

fn parse_lunar_yearly(input: &str, language: Language) -> Result<(u8, u8, bool, u8, u8), String> {
    let mut parts = input.split_whitespace().collect::<Vec<_>>();
    let leap = parts.first().is_some_and(|part| {
        part.eq_ignore_ascii_case("leap")
            || part.eq_ignore_ascii_case(i18n::text(language, key::LEAP))
    });
    if leap {
        parts.remove(0);
    }
    let (month_text, day_text, time_text) = match parts.as_slice() {
        [month, day, time] => (*month, *day, *time),
        [date, time] => {
            let (month, day) = date
                .split_once(['-', '/'])
                .ok_or_else(|| lunar_input_error(language))?;
            (month, day, *time)
        }
        _ => return Err(lunar_input_error(language)),
    };
    let month = month_text
        .parse::<u8>()
        .ok()
        .filter(|month| (1..=12).contains(month))
        .ok_or_else(|| i18n::text(language, key::LUNAR_MONTH_RANGE).to_string())?;
    let day = day_text
        .parse::<u8>()
        .ok()
        .filter(|day| (1..=30).contains(day))
        .ok_or_else(|| i18n::text(language, key::LUNAR_DAY_RANGE).to_string())?;
    let (hour, minute) = parse_time(time_text, language)?;
    Ok((month, day, leap, hour, minute))
}

fn lunar_input_error(language: Language) -> String {
    i18n::text(language, key::LUNAR_TIME_FORMAT).into()
}

fn parse_month(input: &str) -> Option<u8> {
    let normalized = input.trim().to_lowercase();
    if let Ok(month) = normalized.parse::<u8>()
        && (1..=12).contains(&month)
    {
        return Some(month);
    }
    if let Some(numeric) = normalized
        .strip_suffix('月')
        .or_else(|| normalized.strip_suffix('월'))
        && let Ok(month) = numeric.parse::<u8>()
        && (1..=12).contains(&month)
    {
        return Some(month);
    }
    match normalized.as_str() {
        "jan" | "january" | "januar" | "janv" | "janvier" | "ene" | "enero" => Some(1),
        "feb" | "february" | "februar" | "févr" | "fevr" | "février" | "fevrier" | "febrero" => {
            Some(2)
        }
        "mar" | "march" | "mär" | "märz" | "marz" | "mars" | "marzo" => Some(3),
        "apr" | "april" | "avr" | "avril" | "abr" | "abril" => Some(4),
        "may" | "mai" | "mayo" => Some(5),
        "jun" | "june" | "juni" | "juin" | "junio" => Some(6),
        "jul" | "july" | "juli" | "juil" | "juillet" | "julio" => Some(7),
        "aug" | "august" | "août" | "aout" | "ago" | "agosto" => Some(8),
        "sep" | "sept" | "september" | "septiembre" => Some(9),
        "oct" | "october" | "okt" | "oktober" | "octobre" | "octubre" => Some(10),
        "nov" | "november" | "novembre" | "noviembre" => Some(11),
        "dec" | "december" | "dez" | "dezember" | "déc" | "décembre" | "decembre" | "dic"
        | "diciembre" => Some(12),
        _ => None,
    }
}

fn weekday_name(value: u8, language: Language) -> Option<&'static str> {
    let names = match language.effective() {
        Language::System | Language::English => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        Language::SimplifiedChinese => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
        Language::TraditionalChinese => ["週一", "週二", "週三", "週四", "週五", "週六", "週日"],
        Language::Japanese => ["月", "火", "水", "木", "金", "土", "日"],
        Language::Korean => ["월", "화", "수", "목", "금", "토", "일"],
        Language::Spanish => ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
        Language::French => ["lun", "mar", "mer", "jeu", "ven", "sam", "dim"],
        Language::German => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"],
    };
    names.get(value as usize).copied()
}

fn weekday_list_label(weekdays: &[u8], language: Language) -> String {
    let names = weekdays
        .iter()
        .filter_map(|weekday| weekday_name(*weekday, language))
        .collect::<Vec<_>>();
    if names.is_empty() {
        i18n::text(language, key::SELECTED_DAYS).into()
    } else {
        names.join(", ")
    }
}

fn month_name(value: u8, language: Language) -> Option<String> {
    let index = value.checked_sub(1)? as usize;
    let name = match language.effective() {
        Language::System | Language::English => [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ][index]
            .to_string(),
        Language::SimplifiedChinese | Language::TraditionalChinese | Language::Japanese => {
            format!("{value}月")
        }
        Language::Korean => format!("{value}월"),
        Language::Spanish => [
            "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
        ][index]
            .to_string(),
        Language::French => [
            "janv", "févr", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov",
            "déc",
        ][index]
            .to_string(),
        Language::German => [
            "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
        ][index]
            .to_string(),
    };
    Some(name)
}

fn interval_label(seconds: u64, language: Language) -> String {
    for (unit_seconds, singular, plural) in [
        (7 * 86_400, key::UNIT_WEEK, key::UNIT_WEEKS),
        (86_400, key::UNIT_DAY, key::UNIT_DAYS),
        (3_600, key::UNIT_HOUR, key::UNIT_HOURS),
        (60, key::UNIT_MINUTE, key::UNIT_MINUTES),
    ] {
        if seconds >= unit_seconds && seconds % unit_seconds == 0 {
            let count = seconds / unit_seconds;
            let unit = i18n::text(language, if count == 1 { singular } else { plural });
            return if language.is_cjk() {
                if count == 1 {
                    format!("{}{unit}", i18n::text(language, key::EVERY))
                } else {
                    format!("{}{count}{unit}", i18n::text(language, key::EVERY))
                }
            } else if count == 1 {
                format!("{} {unit}", i18n::text(language, key::EVERY))
            } else {
                format!("{} {count} {unit}", i18n::text(language, key::EVERY))
            };
        }
    }
    format!(
        "{} {seconds} {}",
        i18n::text(language, key::EVERY),
        i18n::text(language, key::UNIT_SECONDS)
    )
}

fn add_interval(timestamp: i64, seconds: u64) -> Option<i64> {
    timestamp.checked_add(i64::try_from(seconds).ok()?)
}

fn next_interval_after(after: i64, due: i64, seconds: u64) -> Option<i64> {
    let seconds = i64::try_from(seconds).ok().filter(|seconds| *seconds > 0)?;
    if due > after {
        return Some(due);
    }
    let elapsed = after.checked_sub(due)?;
    let steps = elapsed.checked_div(seconds)?.checked_add(1)?;
    due.checked_add(steps.checked_mul(seconds)?)
}

fn next_matching(
    after: i64,
    hour: u8,
    minute: u8,
    matches_day: impl Fn(NaiveDate) -> bool,
    days_to_check: u64,
) -> Option<i64> {
    let local_after = local_time(after);
    for offset in 0..=days_to_check {
        let date = local_after
            .date_naive()
            .checked_add_days(Days::new(offset))?;
        if !matches_day(date) {
            continue;
        }
        if let Some(candidate) = local_timestamp_after(date, hour, minute, after) {
            return Some(candidate);
        }
    }
    None
}

fn next_lunar_yearly(
    after: i64,
    month: u8,
    day: u8,
    leap: bool,
    hour: u8,
    minute: u8,
) -> Option<i64> {
    let start_year = (local_time(after).year() - 1).clamp(MIN_LUNAR_YEAR, MAX_LUNAR_YEAR);
    for year in start_year..=MAX_LUNAR_YEAR {
        let Ok(date) = LunisolarDate::from_ymd(year as u16, month, leap, day) else {
            continue;
        };
        let date = date.to_naive_date();
        if let Some(candidate) = local_timestamp_after(date, hour, minute, after) {
            return Some(candidate);
        }
    }
    None
}

fn local_timestamp_after(date: NaiveDate, hour: u8, minute: u8, after: i64) -> Option<i64> {
    let naive = date.and_hms_opt(hour as u32, minute as u32, 0)?;
    for candidate in [
        Local.from_local_datetime(&naive).earliest(),
        Local.from_local_datetime(&naive).latest(),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.timestamp() > after {
            return Some(candidate.timestamp());
        }
    }
    None
}

fn local_time(timestamp: i64) -> chrono::DateTime<Local> {
    Local
        .timestamp_opt(timestamp, 0)
        .earliest()
        .unwrap_or_else(Local::now)
}

fn reminder_hud_text(reminder: &Reminder, now: i64, language: Language) -> String {
    let seconds = reminder.next_at.saturating_sub(now);
    let when = if seconds <= 0 {
        i18n::text(language, key::DUE).into()
    } else if seconds < 60 * 60 {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    } else if seconds < 24 * 60 * 60 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if language.is_cjk() {
            format!(
                "{hours}{} {minutes}{}",
                i18n::text(language, key::UNIT_HOURS),
                i18n::text(language, key::UNIT_MINUTES)
            )
        } else {
            format!("{hours}h {minutes}m")
        }
    } else {
        let date = local_time(reminder.next_at);
        let weekday = date.weekday().num_days_from_monday() as u8;
        format!(
            "{} {}",
            weekday_name(weekday, language).unwrap_or(""),
            date.format("%H:%M")
        )
    };
    format!("{}  /  {when}", reminder.text)
}

fn clean_text(input: &str, max_chars: usize, language: Language) -> Result<String, String> {
    let text = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(i18n::text(language, key::TEXT_EMPTY).into());
    }
    if text.chars().count() > max_chars {
        return Err(i18n::template(
            language,
            key::TEXT_TOO_LONG,
            &[("max", max_chars.to_string())],
        ));
    }
    Ok(text)
}

fn remove_by_id<T>(items: &mut Vec<T>, id: u64, get_id: impl Fn(&T) -> u64) -> bool {
    let before = items.len();
    items.retain(|item| get_id(item) != id);
    items.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_one_time_reminder_waits_for_acknowledgement() {
        let mut items = PersonalItems::default();
        items
            .add_reminder(
                ReminderPreset::AfterMinutes,
                "Stretch",
                "25",
                1_000,
                Language::English,
            )
            .unwrap();
        let id = items.reminders[0].id;
        assert_eq!(items.reminders[0].next_at, 2_500);
        assert!(!items.hud_content(2_499, false, Language::English).items[0].due);
        assert!(!items.acknowledge_due(id, 2_499));
        let due = items.hud_content(2_500, false, Language::English);
        assert!(due.items[0].due);
        assert!(due.items[0].flashing);
        assert!(!items.hud_content(2_501, false, Language::English).items[0].flashing);
        assert_eq!(items.reminders.len(), 1);

        assert!(items.acknowledge_due(id, 2_500));
        assert!(items.reminders.is_empty());
    }

    #[test]
    fn notes_are_trimmed_and_bounded() {
        let mut items = PersonalItems::default();
        items
            .add_note("  ship   the patch  ", Language::English)
            .unwrap();
        assert_eq!(items.notes[0].text, "ship the patch");
        assert!(
            items
                .add_note(&"x".repeat(MAX_NOTE_CHARS + 1), Language::English)
                .is_err()
        );
    }

    #[test]
    fn parses_weekdays_and_clock_times() {
        assert_eq!(parse_weekday("Thursday"), Some(3));
        assert_eq!(parse_time("18:30", Language::English).unwrap(), (18, 30));
        assert!(parse_time("25:00", Language::English).is_err());
    }

    #[test]
    fn delayed_hours_use_the_requested_unit() {
        let mut items = PersonalItems::default();
        items
            .add_reminder(
                ReminderPreset::AfterHours,
                "Check build",
                "3",
                1_000,
                Language::English,
            )
            .unwrap();
        assert_eq!(items.reminders[0].next_at, 1_000 + 3 * 3_600);
        assert!(matches!(items.reminders[0].recurrence, Recurrence::Once));
    }

    #[test]
    fn interval_reminders_keep_their_original_cadence_when_delivery_is_late() {
        let mut items = PersonalItems::default();
        items
            .add_reminder(
                ReminderPreset::EveryMinutes,
                "Posture",
                "5",
                1_000,
                Language::English,
            )
            .unwrap();
        assert_eq!(items.reminders[0].next_at, 1_300);

        let id = items.reminders[0].id;
        assert!(items.acknowledge_due(id, 1_905));
        assert_eq!(items.reminders[0].next_at, 2_200);
        assert_eq!(
            items.reminders[0].recurrence.label(Language::English),
            "Every 5 minutes"
        );
    }

    #[test]
    fn selected_weekdays_accept_lists_and_ranges() {
        let (days, hour, minute) =
            parse_weekdays_and_time("Mon,Wed,Fri-Sun 18:30", Language::English).unwrap();
        assert_eq!(days, vec![0, 2, 4, 5, 6]);
        assert_eq!((hour, minute), (18, 30));
    }

    #[test]
    fn schedule_input_accepts_localized_names_and_errors() {
        let (days, hour, minute) =
            parse_weekdays_and_time("周一,周三,周五 18:30", Language::SimplifiedChinese).unwrap();
        assert_eq!(days, vec![0, 2, 4]);
        assert_eq!((hour, minute), (18, 30));
        assert_eq!(
            parse_yearly("déc 31 09:00", Language::French).unwrap(),
            (12, 31, 9, 0)
        );
        assert_eq!(
            parse_lunar_yearly("闰 4 1 18:30", Language::SimplifiedChinese).unwrap(),
            (4, 1, true, 18, 30)
        );
        assert_eq!(
            parse_time("25:00", Language::SimplifiedChinese).unwrap_err(),
            "请输入有效的 24 小时时间。"
        );
    }

    #[test]
    fn identifiers_remain_unique_after_deletion() {
        let mut items = PersonalItems::default();
        items.add_note("one", Language::English).unwrap();
        let first = items.notes[0].id;
        items.remove_note(first);
        items.add_note("two", Language::English).unwrap();
        assert_ne!(items.notes[0].id, first);
    }

    #[test]
    fn recurring_reminders_advance_after_acknowledgement() {
        let now = Local::now().timestamp();
        let mut items = PersonalItems {
            reminders: vec![Reminder {
                id: 1,
                text: "Stand up".into(),
                next_at: now - 1,
                recurrence: Recurrence::Daily {
                    hour: 10,
                    minute: 30,
                },
            }],
            ..PersonalItems::default()
        };

        assert!(items.acknowledge_due(1, now));
        assert!(items.reminders[0].next_at > now);
        assert!(items.reminders[0].next_at <= now + 26 * 60 * 60);
    }

    #[test]
    fn completing_the_next_occurrence_can_skip_a_future_reminder() {
        let mut items = PersonalItems::default();
        items
            .add_reminder(
                ReminderPreset::EveryMinutes,
                "Break",
                "5",
                1_000,
                Language::English,
            )
            .unwrap();
        let id = items.reminders[0].id;

        assert!(items.complete_next(id, 1_100));
        assert_eq!(items.reminders[0].next_at, 1_600);
    }

    #[test]
    fn weekend_schedule_lands_on_saturday_or_sunday() {
        let now = Local::now().timestamp();
        let next = Recurrence::Weekends { hour: 9, minute: 0 }
            .next_after(now)
            .unwrap();
        assert!(matches!(
            local_time(next).weekday(),
            Weekday::Sat | Weekday::Sun
        ));
    }

    #[test]
    fn weekday_schedule_never_lands_on_a_weekend() {
        let now = Local::now().timestamp();
        let next = Recurrence::Weekdays { hour: 9, minute: 0 }
            .next_after(now)
            .unwrap();
        assert!(!matches!(
            local_time(next).weekday(),
            Weekday::Sat | Weekday::Sun
        ));
    }

    #[test]
    fn monthly_schedule_uses_the_requested_day() {
        let now = Local::now().timestamp();
        let next = Recurrence::Monthly {
            day: 15,
            hour: 9,
            minute: 0,
        }
        .next_after(now)
        .unwrap();
        assert_eq!(local_time(next).day(), 15);
    }

    #[test]
    fn month_end_schedule_uses_the_last_calendar_day() {
        let now = Local::now().timestamp();
        let next = Recurrence::MonthlyLast { hour: 9, minute: 0 }
            .next_after(now)
            .unwrap();
        let date = local_time(next).date_naive();
        assert_ne!(
            date.month(),
            date.checked_add_days(Days::new(1)).unwrap().month()
        );
    }

    #[test]
    fn yearly_schedule_accepts_named_and_numeric_dates() {
        assert_eq!(
            parse_yearly("Dec 31 09:00", Language::English).unwrap(),
            (12, 31, 9, 0)
        );
        assert_eq!(
            parse_yearly("2-29 18:30", Language::English).unwrap(),
            (2, 29, 18, 30)
        );
        assert!(parse_yearly("Feb 30 09:00", Language::English).is_err());

        let now = Local::now().timestamp();
        let next = Recurrence::Yearly {
            month: 12,
            day: 31,
            hour: 9,
            minute: 0,
        }
        .next_after(now)
        .unwrap();
        assert_eq!((local_time(next).month(), local_time(next).day()), (12, 31));
    }

    #[test]
    fn lunar_yearly_schedule_uses_the_requested_lunar_date() {
        assert_eq!(
            parse_lunar_yearly("8 15 09:00", Language::English).unwrap(),
            (8, 15, false, 9, 0)
        );
        assert_eq!(
            parse_lunar_yearly("Leap 4 1 18:30", Language::English).unwrap(),
            (4, 1, true, 18, 30)
        );
        assert!(parse_lunar_yearly("13 1 09:00", Language::English).is_err());
        assert!(parse_lunar_yearly("1 31 09:00", Language::English).is_err());

        let after = Local
            .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
            .earliest()
            .unwrap()
            .timestamp();
        let next = Recurrence::LunarYearly {
            month: 1,
            day: 1,
            leap: false,
            hour: 9,
            minute: 0,
        }
        .next_after(after)
        .unwrap();
        assert_eq!(
            local_time(next).date_naive(),
            NaiveDate::from_ymd_opt(2024, 2, 10).unwrap()
        );

        let before_leap_month = Local
            .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
            .earliest()
            .unwrap()
            .timestamp();
        let leap_month = Recurrence::LunarYearly {
            month: 4,
            day: 1,
            leap: true,
            hour: 9,
            minute: 0,
        }
        .next_after(before_leap_month)
        .unwrap();
        assert_eq!(
            local_time(leap_month).date_naive(),
            NaiveDate::from_ymd_opt(2020, 5, 23).unwrap()
        );
    }

    #[test]
    fn imminent_reminder_takes_priority_on_the_hud() {
        let now = 10_000;
        let items = PersonalItems {
            notes: vec![QuickNote {
                id: 1,
                text: "Ordinary note".into(),
            }],
            reminders: vec![Reminder {
                id: 2,
                text: "Focus ends".into(),
                next_at: now + 4 * 60,
                recurrence: Recurrence::Once,
            }],
            ..PersonalItems::default()
        };
        let content = items.hud_content(now, false, Language::English);
        assert_eq!(content.items[0].text, "Ordinary note");
        assert!(content.items[1].text.starts_with("Focus ends"));
        assert!(content.items[1].urgent);
    }

    #[test]
    fn reminders_are_sorted_by_urgency_below_notes() {
        let now = 10_000;
        let items = PersonalItems {
            notes: vec![QuickNote {
                id: 1,
                text: "Pinned note".into(),
            }],
            reminders: vec![
                Reminder {
                    id: 2,
                    text: "Later".into(),
                    next_at: now + 3_600,
                    recurrence: Recurrence::Once,
                },
                Reminder {
                    id: 3,
                    text: "Overdue".into(),
                    next_at: now - 60,
                    recurrence: Recurrence::Once,
                },
                Reminder {
                    id: 4,
                    text: "Soon".into(),
                    next_at: now + 60,
                    recurrence: Recurrence::Once,
                },
            ],
            ..PersonalItems::default()
        };

        let content = items.hud_content(now, false, Language::English);
        assert_eq!(content.items[0].text, "Pinned note");
        assert!(content.items[1].text.starts_with("Overdue"));
        assert!(content.items[1].due);
        assert!(content.items[2].text.starts_with("Soon"));
        assert!(content.items[3].text.starts_with("Later"));
    }

    #[test]
    fn every_personal_item_is_exposed_to_the_hud() {
        let now = 10_000;
        let items = PersonalItems {
            notes: vec![
                QuickNote {
                    id: 1,
                    text: "First note".into(),
                },
                QuickNote {
                    id: 2,
                    text: "Second note".into(),
                },
            ],
            reminders: vec![Reminder {
                id: 3,
                text: "Stand up".into(),
                next_at: now + 10 * 60,
                recurrence: Recurrence::Once,
            }],
            ..PersonalItems::default()
        };

        let content = items.hud_content(now, false, Language::English);
        assert_eq!(content.items.len(), 3);
        assert_eq!(content.items[0].text, "First note");
        assert_eq!(content.items[1].text, "Second note");
        assert_eq!(content.items[2].kind, HudItemKind::Reminder);
    }

    #[test]
    fn distant_reminders_are_hidden_unless_enabled() {
        let now = 10_000;
        let items = PersonalItems {
            notes: vec![QuickNote {
                id: 1,
                text: "Visible note".into(),
            }],
            reminders: vec![
                Reminder {
                    id: 2,
                    text: "At threshold".into(),
                    next_at: now + DISTANT_REMINDER_SECONDS,
                    recurrence: Recurrence::Once,
                },
                Reminder {
                    id: 3,
                    text: "Beyond threshold".into(),
                    next_at: now + DISTANT_REMINDER_SECONDS + 1,
                    recurrence: Recurrence::Once,
                },
            ],
            ..PersonalItems::default()
        };

        let filtered = items.hud_content(now, false, Language::English);
        assert_eq!(filtered.items.len(), 2);
        assert_eq!(filtered.items[0].text, "Visible note");
        assert!(filtered.items[1].text.starts_with("At threshold"));

        let all = items.hud_content(now, true, Language::English);
        assert_eq!(all.items.len(), 3);
        assert!(
            all.items
                .iter()
                .any(|item| item.text.starts_with("Beyond threshold"))
        );
    }
}
