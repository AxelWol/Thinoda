//! Versioned date-only task scheduling and work-calendar arithmetic.

use crate::model::{Sheet, TopicId};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

pub const SCHEDULING_VERSION: u16 = 1;
pub const MAX_SCHEDULED_TASKS: usize = 5_000;
pub const MAX_TASK_DEPENDENCIES: usize = 20_000;
pub const MAX_CALENDAR_EXCEPTIONS: usize = 5_000;
pub const MAX_TASK_DURATION_DAYS: u16 = 3_650;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDate {
    year: u16,
    month: u8,
    day: u8,
}

impl Serialize for LocalDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!(
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        ))
    }
}

impl fmt::Display for LocalDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

impl FromStr for LocalDate {
    type Err = ScheduleError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let bytes = raw.as_bytes();
        if !bytes.is_ascii() || bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(ScheduleError::InvalidDateFormat);
        }
        let year = raw[0..4]
            .parse()
            .map_err(|_| ScheduleError::InvalidDateFormat)?;
        let month = raw[5..7]
            .parse()
            .map_err(|_| ScheduleError::InvalidDateFormat)?;
        let day = raw[8..10]
            .parse()
            .map_err(|_| ScheduleError::InvalidDateFormat)?;
        Self::new(year, month, day)
    }
}

impl<'de> Deserialize<'de> for LocalDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl LocalDate {
    pub const MIN_YEAR: u16 = 1;
    pub const MAX_YEAR: u16 = 9999;
    pub const MIN: Self = Self {
        year: 1,
        month: 1,
        day: 1,
    };
    pub const MAX: Self = Self {
        year: 9999,
        month: 12,
        day: 31,
    };

    pub fn new(year: u16, month: u8, day: u8) -> Result<Self, ScheduleError> {
        if !(Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year)
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
        {
            return Err(ScheduleError::InvalidDate { year, month, day });
        }
        Ok(Self { year, month, day })
    }

    pub fn year(self) -> u16 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    pub fn weekday(self) -> Weekday {
        Weekday::from_index(self.ordinal() % 7)
    }

    /// Day index from `0001-01-01`, which is ordinal zero.
    pub fn ordinal(self) -> u32 {
        days_before_year(self.year)
            + (1..self.month)
                .map(|month| u32::from(days_in_month(self.year, month)))
                .sum::<u32>()
            + u32::from(self.day - 1)
    }

    /// Signed number of calendar days from this date until `other`.
    pub fn days_until(self, other: Self) -> i32 {
        other.ordinal() as i32 - self.ordinal() as i32
    }

    pub fn checked_add_days(self, days: i32) -> Result<Self, ScheduleError> {
        let ordinal = i64::from(self.ordinal()) + i64::from(days);
        if !(0..=i64::from(MAX_ORDINAL)).contains(&ordinal) {
            return Err(ScheduleError::DateOutOfRange);
        }
        Self::from_ordinal(ordinal as u32)
    }

    fn from_ordinal(ordinal: u32) -> Result<Self, ScheduleError> {
        if ordinal > MAX_ORDINAL {
            return Err(ScheduleError::DateOutOfRange);
        }
        let mut low = 1u16;
        let mut high = 10_000u16;
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            if days_before_year(mid) <= ordinal {
                low = mid;
            } else {
                high = mid;
            }
        }
        let mut day_of_year = ordinal - days_before_year(low);
        let mut month = 1;
        while day_of_year >= u32::from(days_in_month(low, month)) {
            day_of_year -= u32::from(days_in_month(low, month));
            month += 1;
        }
        Self::new(low, month, (day_of_year + 1) as u8)
    }
}

const MAX_ORDINAL: u32 = 3_652_058;

fn days_before_year(year: u16) -> u32 {
    let y = u32::from(year - 1);
    365 * y + y / 4 - y / 100 + y / 400
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Monday => "Monday",
            Self::Tuesday => "Tuesday",
            Self::Wednesday => "Wednesday",
            Self::Thursday => "Thursday",
            Self::Friday => "Friday",
            Self::Saturday => "Saturday",
            Self::Sunday => "Sunday",
        }
    }

    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Monday,
            1 => Self::Tuesday,
            2 => Self::Wednesday,
            3 => Self::Thursday,
            4 => Self::Friday,
            5 => Self::Saturday,
            _ => Self::Sunday,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkCalendar {
    pub working_weekdays: BTreeSet<Weekday>,
    /// Each value explicitly overrides the weekday rule: `true` is working.
    pub exceptions: BTreeMap<LocalDate, bool>,
}

impl Default for WorkCalendar {
    fn default() -> Self {
        Self {
            working_weekdays: [
                Weekday::Monday,
                Weekday::Tuesday,
                Weekday::Wednesday,
                Weekday::Thursday,
                Weekday::Friday,
            ]
            .into(),
            exceptions: BTreeMap::new(),
        }
    }
}

impl WorkCalendar {
    pub fn validate(&self) -> Result<(), ScheduleError> {
        if self.working_weekdays.is_empty() {
            return Err(ScheduleError::NoWorkingDays);
        }
        if self.exceptions.len() > MAX_CALENDAR_EXCEPTIONS {
            return Err(ScheduleError::TooManyCalendarExceptions);
        }
        Ok(())
    }

    pub fn is_working_day(&self, date: LocalDate) -> bool {
        self.exceptions
            .get(&date)
            .copied()
            .unwrap_or_else(|| self.working_weekdays.contains(&date.weekday()))
    }

    /// Duration counts working days inclusively: a one-day task starts and finishes
    /// on the same working date.
    pub fn finish_date(
        &self,
        start: LocalDate,
        duration_days: u16,
    ) -> Result<LocalDate, ScheduleError> {
        self.validate()?;
        if duration_days == 0 || duration_days > MAX_TASK_DURATION_DAYS {
            return Err(ScheduleError::InvalidDuration(duration_days));
        }
        if !self.is_working_day(start) {
            return Err(ScheduleError::StartIsNotWorkingDay(start));
        }
        self.add_working_days(start, duration_days - 1)
    }

    /// Counts working dates from `start` through `finish`, inclusive.
    pub fn working_duration(
        &self,
        start: LocalDate,
        finish: LocalDate,
    ) -> Result<u16, ScheduleError> {
        self.validate()?;
        if finish < start {
            return Err(ScheduleError::InvertedDateRange { start, finish });
        }
        if !self.is_working_day(start) {
            return Err(ScheduleError::StartIsNotWorkingDay(start));
        }
        if !self.is_working_day(finish) {
            return Err(ScheduleError::FinishIsNotWorkingDay(finish));
        }
        let count = self.count_working_days_inclusive(start.ordinal(), finish.ordinal())?;
        if count > u32::from(MAX_TASK_DURATION_DAYS) {
            return Err(ScheduleError::DurationOutOfRange(count));
        }
        Ok(count as u16)
    }

    /// Advances by working days after `date`; zero returns `date`.
    pub fn add_working_days(&self, date: LocalDate, days: u16) -> Result<LocalDate, ScheduleError> {
        self.validate()?;
        if days > MAX_TASK_DURATION_DAYS {
            return Err(ScheduleError::InvalidDuration(days));
        }
        if days == 0 {
            return Ok(date);
        }
        let start = date.ordinal();
        let target_count = u32::from(days);
        let max_span = target_count * 7 + self.exceptions.len() as u32 + 7;
        let mut low = start + 1;
        let mut high = start.saturating_add(max_span).min(MAX_ORDINAL);
        if self.count_working_days_after(start, high)? < target_count {
            return Err(ScheduleError::DateOutOfRange);
        }
        while low < high {
            let mid = low + (high - low) / 2;
            if self.count_working_days_after(start, mid)? >= target_count {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        LocalDate::from_ordinal(low)
    }

    fn count_working_days_after(&self, start: u32, end: u32) -> Result<u32, ScheduleError> {
        if end <= start {
            return Ok(0);
        }
        self.count_working_days_inclusive(start + 1, end)
    }

    fn count_working_days_inclusive(&self, start: u32, end: u32) -> Result<u32, ScheduleError> {
        if end < start {
            return Ok(0);
        }
        let span = end - start + 1;
        let full_weeks = span / 7;
        let mut count = full_weeks * self.working_weekdays.len() as u32;
        let remainder = span % 7;
        let first_weekday = start % 7;
        for offset in 0..remainder {
            if self
                .working_weekdays
                .contains(&Weekday::from_index((first_weekday + offset) % 7))
            {
                count += 1;
            }
        }
        let first_date = LocalDate::from_ordinal(start)?;
        let end_date = LocalDate::from_ordinal(end)?;
        for (exception_date, is_working) in self.exceptions.range(first_date..=end_date) {
            let weekday_working = self.working_weekdays.contains(&exception_date.weekday());
            count = count - u32::from(weekday_working) + u32::from(*is_working);
        }
        Ok(count)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub start_date: Option<LocalDate>,
    /// Working-day count, inclusive of the start and finish dates.
    pub duration_days: Option<u16>,
    pub progress_percent: u8,
    /// Optional priority value; interpretation is left to the app's task UI.
    pub priority: Option<u8>,
}

impl Task {
    pub fn validate(&self) -> Result<(), ScheduleError> {
        if let Some(duration) = self.duration_days
            && (duration == 0 || duration > MAX_TASK_DURATION_DAYS)
        {
            return Err(ScheduleError::InvalidDuration(duration));
        }
        if self.progress_percent > 100 {
            return Err(ScheduleError::InvalidProgress(self.progress_percent));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDependency {
    pub predecessor: TopicId,
    pub successor: TopicId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulingData {
    pub version: u16,
    pub calendar: WorkCalendar,
    pub tasks: BTreeMap<TopicId, Task>,
    pub dependencies: Vec<TaskDependency>,
}

impl Default for SchedulingData {
    fn default() -> Self {
        Self {
            version: SCHEDULING_VERSION,
            calendar: WorkCalendar::default(),
            tasks: BTreeMap::new(),
            dependencies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedTask {
    pub start_date: Option<LocalDate>,
    pub finish_date: Option<LocalDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("date must use YYYY-MM-DD format")]
    InvalidDateFormat,
    #[error("invalid date {year:04}-{month:02}-{day:02}")]
    InvalidDate { year: u16, month: u8, day: u8 },
    #[error("date is outside the supported range 0001-01-01 through 9999-12-31")]
    DateOutOfRange,
    #[error("unsupported scheduling data version {0}")]
    UnsupportedVersion(u16),
    #[error("calendar must contain at least one working weekday")]
    NoWorkingDays,
    #[error("calendar has too many dated exceptions")]
    TooManyCalendarExceptions,
    #[error("there are too many scheduled tasks")]
    TooManyTasks,
    #[error("there are too many task dependencies")]
    TooManyDependencies,
    #[error("task duration {0} is outside the supported range 1..={MAX_TASK_DURATION_DAYS}")]
    InvalidDuration(u16),
    #[error("task progress {0} is outside the supported range 0..=100")]
    InvalidProgress(u8),
    #[error("task starts on nonworking date {0:?}")]
    StartIsNotWorkingDay(LocalDate),
    #[error("task finishes on nonworking date {0:?}")]
    FinishIsNotWorkingDay(LocalDate),
    #[error("finish date {finish} precedes start date {start}")]
    InvertedDateRange { start: LocalDate, finish: LocalDate },
    #[error("working-day duration {0} exceeds the supported range 1..={MAX_TASK_DURATION_DAYS}")]
    DurationOutOfRange(u32),
    #[error("task {0} does not exist")]
    MissingTask(TopicId),
    #[error("topic {0} does not exist")]
    MissingTopic(TopicId),
    #[error("dependency from task {0} references a missing task")]
    DanglingDependency(TopicId),
    #[error("task {0} cannot depend on itself")]
    SelfDependency(TopicId),
    #[error("task {0} has a duplicate dependency")]
    DuplicateDependency(TopicId),
    #[error("task dependencies contain a cycle")]
    DependencyCycle,
    #[error("task dependency was not found")]
    DependencyNotFound,
    #[error("task {0} needs a start date before it can schedule its successors")]
    MissingStartDate(TopicId),
    #[error("task {0} needs a duration before it can schedule its successors")]
    MissingDuration(TopicId),
}

impl SchedulingData {
    pub fn validate(&self, sheet: &Sheet) -> Result<(), ScheduleError> {
        if self.version != SCHEDULING_VERSION {
            return Err(ScheduleError::UnsupportedVersion(self.version));
        }
        self.calendar.validate()?;
        if self.tasks.len() > MAX_SCHEDULED_TASKS {
            return Err(ScheduleError::TooManyTasks);
        }
        if self.dependencies.len() > MAX_TASK_DEPENDENCIES {
            return Err(ScheduleError::TooManyDependencies);
        }
        for (topic_id, task) in &self.tasks {
            if !sheet.contains(*topic_id) {
                return Err(ScheduleError::MissingTask(*topic_id));
            }
            task.validate()?;
            if let Some(start) = task.start_date
                && !self.calendar.is_working_day(start)
            {
                return Err(ScheduleError::StartIsNotWorkingDay(start));
            }
        }
        let mut seen = BTreeSet::new();
        let mut outgoing = BTreeMap::<TopicId, Vec<TopicId>>::new();
        let mut indegree = self
            .tasks
            .keys()
            .map(|topic| (*topic, 0usize))
            .collect::<BTreeMap<_, _>>();
        for dependency in &self.dependencies {
            if dependency.predecessor == dependency.successor {
                return Err(ScheduleError::SelfDependency(dependency.predecessor));
            }
            if !self.tasks.contains_key(&dependency.predecessor) {
                return Err(ScheduleError::DanglingDependency(dependency.predecessor));
            }
            if !self.tasks.contains_key(&dependency.successor) {
                return Err(ScheduleError::DanglingDependency(dependency.successor));
            }
            if !seen.insert((dependency.predecessor, dependency.successor)) {
                return Err(ScheduleError::DuplicateDependency(dependency.successor));
            }
            outgoing
                .entry(dependency.predecessor)
                .or_default()
                .push(dependency.successor);
            *indegree.get_mut(&dependency.successor).unwrap() += 1;
        }
        for successors in outgoing.values_mut() {
            successors.sort_unstable();
        }
        let mut ready = indegree
            .iter()
            .filter_map(|(topic, degree)| (*degree == 0).then_some(*topic))
            .collect::<BTreeSet<_>>();
        let mut visited = 0usize;
        while let Some(topic) = ready.pop_first() {
            visited += 1;
            if let Some(successors) = outgoing.get(&topic) {
                for successor in successors {
                    let degree = indegree.get_mut(successor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(*successor);
                    }
                }
            }
        }
        if visited != self.tasks.len() {
            return Err(ScheduleError::DependencyCycle);
        }
        Ok(())
    }

    /// Computes a deterministic, finish-to-start schedule. Durations count
    /// working days inclusively; a successor starts on the next working day
    /// after its predecessor finishes. Earlier successor dates are pushed forward.
    pub fn compute(&self, sheet: &Sheet) -> Result<BTreeMap<TopicId, ComputedTask>, ScheduleError> {
        self.validate(sheet)?;
        let mut outgoing = BTreeMap::<TopicId, Vec<TopicId>>::new();
        let mut indegree = self
            .tasks
            .keys()
            .map(|topic| (*topic, 0usize))
            .collect::<BTreeMap<_, _>>();
        for dependency in &self.dependencies {
            outgoing
                .entry(dependency.predecessor)
                .or_default()
                .push(dependency.successor);
            *indegree.get_mut(&dependency.successor).unwrap() += 1;
        }
        for successors in outgoing.values_mut() {
            successors.sort_unstable();
        }
        let mut ready = indegree
            .iter()
            .filter_map(|(topic, degree)| (*degree == 0).then_some(*topic))
            .collect::<BTreeSet<_>>();
        let mut output = self
            .tasks
            .iter()
            .map(|(topic, task)| {
                (
                    *topic,
                    ComputedTask {
                        start_date: task.start_date,
                        finish_date: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        while let Some(topic) = ready.pop_first() {
            let task = &self.tasks[&topic];
            if let (Some(start), Some(duration)) = (output[&topic].start_date, task.duration_days) {
                output.get_mut(&topic).unwrap().finish_date =
                    Some(self.calendar.finish_date(start, duration)?);
            }
            if let Some(successors) = outgoing.get(&topic) {
                let Some(finish) = output[&topic].finish_date else {
                    if output[&topic].start_date.is_none() {
                        return Err(ScheduleError::MissingStartDate(topic));
                    }
                    return Err(ScheduleError::MissingDuration(topic));
                };
                let earliest = self.calendar.add_working_days(finish, 1)?;
                for successor in successors {
                    let computed = output.get_mut(successor).unwrap();
                    computed.start_date = Some(
                        computed
                            .start_date
                            .map_or(earliest, |current| current.max(earliest)),
                    );
                    let degree = indegree.get_mut(successor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(*successor);
                    }
                }
            }
        }
        Ok(output)
    }
}

impl Sheet {
    pub fn set_task(&mut self, topic: TopicId, task: Option<Task>) -> Result<(), ScheduleError> {
        if !self.contains(topic) {
            return Err(ScheduleError::MissingTopic(topic));
        }
        let previous = self.scheduling.tasks.get(&topic).cloned();
        let previous_dependencies = self.scheduling.dependencies.clone();
        if let Some(task) = task {
            task.validate()?;
            self.scheduling.tasks.insert(topic, task);
        } else {
            self.scheduling.tasks.remove(&topic);
            self.scheduling.dependencies.retain(|dependency| {
                dependency.predecessor != topic && dependency.successor != topic
            });
        }
        if let Err(error) = self.scheduling.validate(self) {
            if let Some(previous) = previous {
                self.scheduling.tasks.insert(topic, previous);
            } else {
                self.scheduling.tasks.remove(&topic);
            }
            self.scheduling.dependencies = previous_dependencies;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_work_calendar(&mut self, calendar: WorkCalendar) -> Result<(), ScheduleError> {
        calendar.validate()?;
        let previous = std::mem::replace(&mut self.scheduling.calendar, calendar);
        if let Err(error) = self.scheduling.validate(self) {
            self.scheduling.calendar = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn add_task_dependency(
        &mut self,
        predecessor: TopicId,
        successor: TopicId,
    ) -> Result<(), ScheduleError> {
        self.scheduling.dependencies.push(TaskDependency {
            predecessor,
            successor,
        });
        if let Err(error) = self.scheduling.validate(self) {
            self.scheduling.dependencies.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn remove_task_dependency(
        &mut self,
        predecessor: TopicId,
        successor: TopicId,
    ) -> Result<(), ScheduleError> {
        let Some(index) = self.scheduling.dependencies.iter().position(|dependency| {
            dependency.predecessor == predecessor && dependency.successor == successor
        }) else {
            return Err(ScheduleError::DependencyNotFound);
        };
        self.scheduling.dependencies.remove(index);
        Ok(())
    }

    pub fn compute_schedule(&self) -> Result<BTreeMap<TopicId, ComputedTask>, ScheduleError> {
        self.scheduling.compute(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: u16, month: u8, day: u8) -> LocalDate {
        LocalDate::new(year, month, day).unwrap()
    }

    #[test]
    fn validates_dates_and_roundtrips_leap_day() {
        let leap = date(2024, 2, 29);
        assert_eq!(leap.weekday(), Weekday::Thursday);
        assert_eq!(leap.to_string(), "2024-02-29");
        assert_eq!("2024-02-29".parse::<LocalDate>().unwrap(), leap);
        assert_eq!(date(2024, 2, 28).days_until(leap), 1);
        assert_eq!(leap.days_until(date(2024, 2, 28)), -1);
        assert_eq!(leap.checked_add_days(1).unwrap(), date(2024, 3, 1));
        assert_eq!(LocalDate::MAX.ordinal(), MAX_ORDINAL);
        assert!(LocalDate::MIN.checked_add_days(-1).is_err());
        assert!(LocalDate::MAX.checked_add_days(1).is_err());
        assert_eq!(
            serde_json::from_str::<LocalDate>(&serde_json::to_string(&leap).unwrap()).unwrap(),
            leap
        );
        assert!(LocalDate::new(2023, 2, 29).is_err());
        assert!(serde_json::from_str::<LocalDate>(r#""2023-02-29""#).is_err());
        assert!(LocalDate::from_str("24-02-29").is_err());
        assert_eq!(
            Weekday::ALL.map(Weekday::name),
            [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ]
        );
    }

    #[test]
    fn finish_to_start_honors_weekends_and_dated_exceptions() {
        let start = date(2024, 2, 29);
        let mut calendar = WorkCalendar::default();
        calendar.exceptions.insert(date(2024, 3, 1), false);
        calendar.exceptions.insert(date(2024, 3, 2), true);
        assert_eq!(calendar.finish_date(start, 2).unwrap(), date(2024, 3, 2));
        assert_eq!(
            calendar.add_working_days(date(2024, 3, 1), 1).unwrap(),
            date(2024, 3, 2)
        );
        assert_eq!(
            calendar.add_working_days(date(2024, 3, 4), 1).unwrap(),
            date(2024, 3, 5)
        );
        assert_eq!(
            calendar.working_duration(start, date(2024, 3, 2)).unwrap(),
            2
        );
        assert_eq!(
            calendar.finish_date(date(2024, 2, 29), 1).unwrap(),
            date(2024, 2, 29)
        );
        assert_eq!(
            WorkCalendar::default()
                .working_duration(date(2024, 2, 29), date(2024, 3, 4))
                .unwrap(),
            3
        );
    }

    #[test]
    fn duration_is_inclusive_and_nonworking_start_is_rejected() {
        let calendar = WorkCalendar::default();
        assert_eq!(
            calendar.finish_date(date(2024, 3, 4), 1).unwrap(),
            date(2024, 3, 4)
        );
        assert!(matches!(
            calendar.finish_date(date(2024, 3, 2), 1),
            Err(ScheduleError::StartIsNotWorkingDay(_))
        ));
        assert!(matches!(
            calendar.working_duration(date(2024, 3, 4), date(2024, 3, 1)),
            Err(ScheduleError::InvertedDateRange { .. })
        ));
        assert!(matches!(
            calendar.working_duration(date(2024, 3, 4), date(2024, 3, 9)),
            Err(ScheduleError::FinishIsNotWorkingDay(_))
        ));
        let every_day = WorkCalendar {
            working_weekdays: Weekday::ALL.into(),
            ..WorkCalendar::default()
        };
        assert!(matches!(
            every_day.finish_date(LocalDate::MAX, 2),
            Err(ScheduleError::DateOutOfRange)
        ));
        assert!(matches!(
            every_day.add_working_days(LocalDate::MIN, MAX_TASK_DURATION_DAYS + 1),
            Err(ScheduleError::InvalidDuration(_))
        ));
    }

    #[test]
    fn finish_to_start_propagation_is_ordered_and_cycles_are_rejected() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let first = sheet.add_child(sheet.root, None, "First");
        let second = sheet.add_child(sheet.root, None, "Second");
        let third = sheet.add_child(sheet.root, None, "Third");
        let start = date(2024, 2, 29);
        sheet
            .set_task(
                first,
                Some(Task {
                    start_date: Some(start),
                    duration_days: Some(2),
                    ..Task::default()
                }),
            )
            .unwrap();
        sheet
            .set_task(
                second,
                Some(Task {
                    start_date: Some(start),
                    duration_days: Some(1),
                    ..Task::default()
                }),
            )
            .unwrap();
        sheet
            .set_task(
                third,
                Some(Task {
                    start_date: Some(date(2024, 2, 29)),
                    duration_days: Some(1),
                    ..Task::default()
                }),
            )
            .unwrap();
        sheet
            .set_work_calendar({
                let mut calendar = WorkCalendar::default();
                calendar.exceptions.insert(date(2024, 3, 1), false);
                calendar
            })
            .unwrap();
        sheet.add_task_dependency(first, second).unwrap();
        sheet.add_task_dependency(second, third).unwrap();
        let computed = sheet.compute_schedule().unwrap();
        assert_eq!(computed[&first].finish_date, Some(date(2024, 3, 4)));
        assert_eq!(computed[&second].start_date, Some(date(2024, 3, 5)));
        assert_eq!(computed[&third].start_date, Some(date(2024, 3, 6)));
        assert!(matches!(
            sheet.add_task_dependency(third, first),
            Err(ScheduleError::DependencyCycle)
        ));
        assert_eq!(sheet.scheduling.dependencies.len(), 2);
        sheet.validate().unwrap();
    }

    #[test]
    fn subtree_duplicate_and_delete_preserve_only_valid_schedule_links() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let first = sheet.add_child(sheet.root, None, "First");
        let child = sheet.add_child(first, None, "Child");
        let outside = sheet.add_child(sheet.root, None, "Outside");
        let task = Task {
            start_date: Some(date(2024, 3, 4)),
            duration_days: Some(1),
            ..Task::default()
        };
        sheet.set_task(first, Some(task.clone())).unwrap();
        sheet.set_task(child, Some(task.clone())).unwrap();
        sheet.set_task(outside, Some(task)).unwrap();
        sheet.add_task_dependency(first, child).unwrap();
        sheet.add_task_dependency(outside, first).unwrap();

        let duplicate = sheet.duplicate(first).unwrap();
        let duplicate_child = sheet.children(duplicate)[0];
        assert!(sheet.scheduling.tasks.contains_key(&duplicate));
        assert!(sheet.scheduling.tasks.contains_key(&duplicate_child));
        assert!(sheet.scheduling.dependencies.contains(&TaskDependency {
            predecessor: duplicate,
            successor: duplicate_child,
        }));
        assert!(!sheet.scheduling.dependencies.iter().any(|dependency| {
            dependency.predecessor == outside && dependency.successor == duplicate
        }));

        let subtree = sheet.copy_subtree(first).unwrap();
        let pasted = sheet.paste(sheet.root, &[subtree]).unwrap()[0];
        let pasted_child = sheet.children(pasted)[0];
        assert!(sheet.scheduling.tasks.contains_key(&pasted));
        assert!(sheet.scheduling.tasks.contains_key(&pasted_child));
        assert!(sheet.scheduling.dependencies.contains(&TaskDependency {
            predecessor: pasted,
            successor: pasted_child,
        }));
        sheet.validate().unwrap();

        sheet.remove_topics(&[first]).unwrap();
        sheet.validate().unwrap();
        assert!(!sheet.scheduling.tasks.contains_key(&first));
        assert!(
            sheet
                .scheduling
                .dependencies
                .iter()
                .all(|dependency| dependency.predecessor != first && dependency.successor != first)
        );
    }

    #[test]
    fn moving_scheduled_subtrees_preserves_internal_and_cross_links_once() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let source = sheet.add_child(sheet.root, None, "Source");
        let target = sheet.add_child(sheet.root, None, "Target");
        let first = sheet.add_child(source, None, "First");
        let second = sheet.add_child(source, None, "Second");
        let task = Task {
            start_date: Some(date(2024, 3, 4)),
            duration_days: Some(1),
            ..Task::default()
        };
        for topic in [source, target, first, second] {
            sheet.set_task(topic, Some(task.clone())).unwrap();
        }
        sheet.add_task_dependency(first, second).unwrap();
        sheet.add_task_dependency(source, first).unwrap();

        sheet.move_topic(first, target, 0).unwrap();
        sheet.validate().unwrap();
        assert_eq!(
            sheet
                .scheduling
                .dependencies
                .iter()
                .filter(|dependency| {
                    dependency.predecessor == first && dependency.successor == second
                })
                .count(),
            1
        );
        assert!(sheet.scheduling.dependencies.contains(&TaskDependency {
            predecessor: source,
            successor: first,
        }));
    }

    #[test]
    fn unsupported_versions_and_dangling_dependencies_fail_validation() {
        let mut sheet = Sheet::new("Schedule", "Root");
        sheet.scheduling.version = SCHEDULING_VERSION + 1;
        assert!(matches!(
            sheet.validate().unwrap_err().as_str(),
            "unsupported scheduling data version 2"
        ));
        sheet.scheduling.version = SCHEDULING_VERSION;
        sheet.scheduling.dependencies.push(TaskDependency {
            predecessor: TopicId(99),
            successor: sheet.root,
        });
        assert!(matches!(
            sheet.validate().unwrap_err().as_str(),
            value if value.contains("dependency") && value.contains("missing task")
        ));
    }

    #[test]
    fn document_task_calendar_and_dependency_changes_undo_as_single_edits() {
        let mut document = crate::Document::default();
        let root = document.sheet().root;
        let topics = document
            .edit_sheet("Add task topics", |sheet| {
                Ok((
                    sheet.insert_child(root, "First")?,
                    sheet.insert_child(root, "Second")?,
                ))
            })
            .unwrap();
        let task = Task {
            start_date: Some(date(2024, 3, 4)),
            duration_days: Some(3),
            progress_percent: 25,
            priority: Some(3),
        };
        document.set_task(topics.0, Some(task.clone())).unwrap();
        document.set_task(topics.1, Some(task)).unwrap();
        document.add_task_dependency(topics.0, topics.1).unwrap();
        assert_eq!(
            document.sheet().compute_schedule().unwrap()[&topics.1].start_date,
            Some(date(2024, 3, 7))
        );
        assert_eq!(document.undo_label(), Some("Add task dependency"));
        assert!(document.undo());
        assert!(document.sheet().scheduling.dependencies.is_empty());
        assert!(document.redo());
        assert_eq!(document.sheet().scheduling.dependencies.len(), 1);

        let original_calendar = document.sheet().scheduling.calendar.clone();
        let mut changed_calendar = original_calendar.clone();
        changed_calendar.working_weekdays.remove(&Weekday::Friday);
        document.set_work_calendar(changed_calendar).unwrap();
        assert_eq!(document.undo_label(), Some("Edit work calendar"));
        assert!(document.undo());
        assert_eq!(document.sheet().scheduling.calendar, original_calendar);
        assert!(document.redo());
        assert!(
            !document
                .sheet()
                .scheduling
                .calendar
                .working_weekdays
                .contains(&Weekday::Friday)
        );
        assert_eq!(
            document.sheet().scheduling.tasks[&topics.0].progress_percent,
            25
        );
    }
}
