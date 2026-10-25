//! Bounded Gantt chart exports built from the core work-calendar schedule.

use std::collections::BTreeMap;

use image::ImageFormat;
use thinoda_core::{LocalDate, Sheet, TaskDependency, TopicId};

use crate::{
    IoError, PrintDocument,
    print::{PrintSettings, render_print_svg},
    render,
};

pub const MAX_GANTT_TIMELINE_DAYS: u32 = 6_000;
pub const MAX_GANTT_ROWS: usize = 5_000;
pub const MAX_GANTT_LABEL_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_GANTT_SVG_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GANTT_CSV_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GANTT_IMAGE_SIDE: u32 = 16_384;
pub const MAX_GANTT_IMAGE_PIXELS: u64 = 32_000_000;

const LABEL_COLUMN_WIDTH: f32 = 520.0;
const DAY_WIDTH: f32 = 14.0;
const ROW_HEIGHT: f32 = 30.0;
const HEADER_HEIGHT: f32 = 72.0;
const ROW_LIMIT: usize = MAX_GANTT_ROWS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GanttBounds {
    pub start: LocalDate,
    pub finish: LocalDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GanttRow {
    pub topic: TopicId,
    pub name: String,
    pub start_date: Option<LocalDate>,
    pub finish_date: Option<LocalDate>,
    pub duration_days: Option<u16>,
    pub progress_percent: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GanttDocument {
    pub svg: String,
    pub bounds: Option<GanttBounds>,
    pub rows: Vec<GanttRow>,
    pub dimensions: [f32; 2],
}

impl GanttDocument {
    pub fn png(&self, scale: f32) -> Result<Vec<u8>, IoError> {
        if !scale.is_finite() || !(0.1..=8.0).contains(&scale) {
            return Err(IoError::Invalid(
                "Gantt PNG scale must be finite and within 0.1..=8".into(),
            ));
        }
        let width = (self.dimensions[0] * scale).ceil();
        let height = (self.dimensions[1] * scale).ceil();
        if width > MAX_GANTT_IMAGE_SIDE as f32 || height > MAX_GANTT_IMAGE_SIDE as f32 {
            return Err(IoError::GanttLimit("PNG image side"));
        }
        if u64::from(width as u32) * u64::from(height as u32) > MAX_GANTT_IMAGE_PIXELS {
            return Err(IoError::GanttLimit("PNG pixel count"));
        }
        render::raster_svg(&self.svg, scale, ImageFormat::Png)
    }

    pub fn pdf(&self) -> Result<Vec<u8>, IoError> {
        render::pdf_svg(&self.svg)
    }

    pub fn print(&self, settings: PrintSettings) -> Result<PrintDocument, IoError> {
        render_print_svg(&self.svg, settings)
    }
}

/// Builds a chart in stable topic-ID order. Each row is retained, including tasks
/// without dates; `bounds` spans every computed date in the sheet's task table.
pub fn render_gantt(sheet: &Sheet) -> Result<GanttDocument, IoError> {
    let rows = scheduled_rows(sheet)?;
    let bounds = date_bounds(&rows);
    let timeline_days = if let Some(bounds) = bounds {
        bounds.start.days_until(bounds.finish) as u32 + 1
    } else {
        0
    };
    if timeline_days > MAX_GANTT_TIMELINE_DAYS {
        return Err(IoError::GanttLimit("timeline length"));
    }
    let timeline_width = if bounds.is_some() {
        timeline_days as f32 * DAY_WIDTH
    } else {
        560.0
    };
    let width = LABEL_COLUMN_WIDTH + timeline_width + 16.0;
    let height = HEADER_HEIGHT + rows.len() as f32 * ROW_HEIGHT + 12.0;
    let mut svg = String::with_capacity((rows.len() * 200).min(MAX_GANTT_SVG_BYTES));
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\">"
    ));
    svg.push_str(&format!("<defs><clipPath id=\"task-label-clip\"><rect x=\"8\" y=\"{HEADER_HEIGHT:.1}\" width=\"170\" height=\"{:.1}\"/></clipPath><marker id=\"dependency-arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"7\" refY=\"3\" orient=\"auto\"><path d=\"M0,0 L0,6 L7,3 z\" fill=\"#607d8b\"/></marker></defs>", rows.len() as f32 * ROW_HEIGHT));
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    svg.push_str("<g font-family=\"sans-serif\" fill=\"#263238\">");
    svg.push_str("<text x=\"12\" y=\"27\" font-size=\"14\" font-weight=\"bold\">Task</text>");
    svg.push_str("<text x=\"190\" y=\"27\" font-size=\"12\" font-weight=\"bold\">Start</text>");
    svg.push_str("<text x=\"270\" y=\"27\" font-size=\"12\" font-weight=\"bold\">Finish</text>");
    svg.push_str("<text x=\"360\" y=\"27\" font-size=\"12\" font-weight=\"bold\">Duration</text>");
    svg.push_str("<text x=\"440\" y=\"27\" font-size=\"12\" font-weight=\"bold\">Progress</text>");
    if let Some(bounds) = bounds {
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"27\" font-size=\"13\" font-weight=\"bold\">{} – {}</text>",
            LABEL_COLUMN_WIDTH, bounds.start, bounds.finish
        ));
    } else {
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"27\" font-size=\"13\" fill=\"#65727a\">No scheduled dates</text>",
            LABEL_COLUMN_WIDTH
        ));
    }
    for (index, _) in rows.iter().enumerate() {
        let top = HEADER_HEIGHT + index as f32 * ROW_HEIGHT;
        let background = if index % 2 == 0 { "#f7f9fa" } else { "#ffffff" };
        svg.push_str(&format!(
            "<rect x=\"0\" y=\"{top:.1}\" width=\"{width:.1}\" height=\"{ROW_HEIGHT:.1}\" fill=\"{background}\"/>"
        ));
    }
    if let Some(bounds) = bounds {
        write_date_grid(&mut svg, bounds, timeline_days, height);
    }
    write_dependency_connectors(&mut svg, sheet, &rows, bounds);
    for (index, row) in rows.iter().enumerate() {
        let center = HEADER_HEIGHT + index as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.0;
        svg.push_str(&format!(
            "<text x=\"12\" y=\"{:.1}\" font-size=\"12\" clip-path=\"url(#task-label-clip)\">{}</text>",
            center + 4.0,
            xml_escape(&row.name)
        ));
        svg.push_str(&format!(
            "<text x=\"190\" y=\"{:.1}\" font-size=\"11\">{}</text>",
            center + 4.0,
            row.start_date
                .map_or_else(String::new, |date| date.to_string())
        ));
        svg.push_str(&format!(
            "<text x=\"270\" y=\"{:.1}\" font-size=\"11\">{}</text>",
            center + 4.0,
            row.finish_date
                .map_or_else(String::new, |date| date.to_string())
        ));
        svg.push_str(&format!(
            "<text x=\"360\" y=\"{:.1}\" font-size=\"11\">{}</text>",
            center + 4.0,
            row.duration_days
                .map_or_else(|| "—".into(), |duration| format!("{duration}d"))
        ));
        svg.push_str(&format!(
            "<text x=\"440\" y=\"{:.1}\" font-size=\"11\">{}%</text>",
            center + 4.0,
            row.progress_percent
        ));
        if let (Some(bounds), Some(start), Some(finish)) = (bounds, row.start_date, row.finish_date)
        {
            let x = LABEL_COLUMN_WIDTH + bounds.start.days_until(start) as f32 * DAY_WIDTH;
            let bar_width = (start.days_until(finish) as f32 + 1.0) * DAY_WIDTH;
            svg.push_str(&format!(
                "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"{bar_width:.1}\" height=\"16\" rx=\"3\" fill=\"#5b9bd5\"/>",
                center - 8.0
            ));
            if row.progress_percent > 0 {
                svg.push_str(&format!(
                    "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"16\" rx=\"3\" fill=\"#2878b5\"/>",
                    center - 8.0,
                    bar_width * row.progress_percent as f32 / 100.0
                ));
            }
        }
    }
    svg.push_str("</g></svg>");
    if svg.len() > MAX_GANTT_SVG_BYTES {
        return Err(IoError::GanttLimit("SVG size"));
    }
    Ok(GanttDocument {
        svg,
        bounds,
        rows,
        dimensions: [width, height],
    })
}

/// Renders the complete task table and timeline to a PNG at the requested scale.
pub fn render_gantt_png(sheet: &Sheet, scale: f32) -> Result<Vec<u8>, IoError> {
    render_gantt(sheet)?.png(scale)
}

/// Renders the complete task table and timeline as a vector PDF.
pub fn render_gantt_pdf(sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    render_gantt(sheet)?.pdf()
}

/// Produces a paper-sized image document for the existing native print pipeline.
pub fn render_gantt_print(
    sheet: &Sheet,
    settings: PrintSettings,
) -> Result<PrintDocument, IoError> {
    render_gantt(sheet)?.print(settings)
}

/// Exports the same deterministic task rows as UTF-8 CSV with CRLF line endings.
pub fn export_gantt_csv(sheet: &Sheet) -> Result<Vec<u8>, IoError> {
    let rows = scheduled_rows(sheet)?;
    let mut csv = String::new();
    append_csv_text(
        &mut csv,
        "Task,Start,Finish,Duration (working days),Progress,Dependencies\r\n",
    )?;
    let dependencies = dependencies_by_successor(sheet);
    for row in &rows {
        append_csv_field(&mut csv, &row.name)?;
        append_csv_text(&mut csv, ",")?;
        append_csv_field(
            &mut csv,
            &row.start_date
                .map_or_else(String::new, |date| date.to_string()),
        )?;
        append_csv_text(&mut csv, ",")?;
        append_csv_field(
            &mut csv,
            &row.finish_date
                .map_or_else(String::new, |date| date.to_string()),
        )?;
        append_csv_text(&mut csv, ",")?;
        append_csv_field(
            &mut csv,
            &row.duration_days
                .map_or_else(String::new, |days| days.to_string()),
        )?;
        append_csv_text(&mut csv, ",")?;
        append_csv_field(&mut csv, &row.progress_percent.to_string())?;
        append_csv_text(&mut csv, ",")?;
        append_csv_dependencies(&mut csv, &dependencies, row.topic, sheet)?;
        append_csv_text(&mut csv, "\r\n")?;
    }
    Ok(csv.into_bytes())
}

fn scheduled_rows(sheet: &Sheet) -> Result<Vec<GanttRow>, IoError> {
    let computed = sheet.compute_schedule()?;
    sheet
        .validate()
        .map_err(|error| IoError::Invalid(format!("invalid Gantt sheet: {error}")))?;
    if sheet.scheduling.tasks.is_empty() {
        return Err(IoError::GanttNoTasks);
    }
    if sheet.scheduling.tasks.len() > ROW_LIMIT {
        return Err(IoError::GanttLimit("row count"));
    }
    let mut rows = sheet
        .scheduling
        .tasks
        .iter()
        .map(|(topic, task)| {
            let projected = computed.get(topic).copied().ok_or_else(|| {
                IoError::Invalid(format!("Gantt schedule did not contain task {}", topic.0))
            })?;
            Ok(GanttRow {
                topic: *topic,
                name: sheet.topics[topic].title.clone(),
                start_date: projected.start_date,
                finish_date: projected.finish_date,
                duration_days: task.duration_days,
                progress_percent: task.progress_percent,
            })
        })
        .collect::<Result<Vec<_>, IoError>>()?;
    rows.sort_by_key(|row| row.topic);
    let label_bytes = rows
        .iter()
        .map(|row| row.name.len())
        .try_fold(0usize, usize::checked_add)
        .ok_or(IoError::GanttLimit("label size"))?;
    if label_bytes > MAX_GANTT_LABEL_BYTES {
        return Err(IoError::GanttLimit("label size"));
    }
    Ok(rows)
}

fn date_bounds(rows: &[GanttRow]) -> Option<GanttBounds> {
    let mut dates = rows
        .iter()
        .flat_map(|row| [row.start_date, row.finish_date].into_iter().flatten());
    let first = dates.next()?;
    let (start, finish) = dates.fold((first, first), |(start, finish), date| {
        (start.min(date), finish.max(date))
    });
    Some(GanttBounds { start, finish })
}

fn write_date_grid(svg: &mut String, bounds: GanttBounds, days: u32, height: f32) {
    let mut date = bounds.start;
    let mut previous_month = None;
    for offset in 0..days {
        if date.day() == 1 || offset == 0 {
            let x = LABEL_COLUMN_WIDTH + offset as f32 * DAY_WIDTH;
            let label = if date.month() == 1 || offset == 0 {
                format!("{}-{:02}", date.year(), date.month())
            } else {
                format!("{:02}", date.month())
            };
            if previous_month != Some((date.year(), date.month())) {
                svg.push_str(&format!(
                    "<line x1=\"{x:.1}\" y1=\"{HEADER_HEIGHT:.1}\" x2=\"{x:.1}\" y2=\"{height:.1}\" stroke=\"#cbd3d8\" stroke-width=\"1\"/>"
                ));
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"57\" font-size=\"10\" fill=\"#53636b\">{label}</text>",
                    x + 2.0
                ));
                previous_month = Some((date.year(), date.month()));
            }
        }
        if offset + 1 < days {
            date = date.checked_add_days(1).expect("date bound is validated");
        }
    }
    let x = LABEL_COLUMN_WIDTH + days as f32 * DAY_WIDTH;
    svg.push_str(&format!(
        "<line x1=\"{x:.1}\" y1=\"{HEADER_HEIGHT:.1}\" x2=\"{x:.1}\" y2=\"{height:.1}\" stroke=\"#cbd3d8\" stroke-width=\"1\"/>"
    ));
}

fn write_dependency_connectors(
    svg: &mut String,
    sheet: &thinoda_core::Sheet,
    rows: &[GanttRow],
    bounds: Option<GanttBounds>,
) {
    let Some(bounds) = bounds else {
        return;
    };
    let row_indices = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.topic, index))
        .collect::<BTreeMap<_, _>>();
    let mut dependencies = sheet.scheduling.dependencies.clone();
    dependencies.sort_by_key(|dependency| (dependency.predecessor, dependency.successor));
    svg.push_str("<g fill=\"none\" stroke=\"#607d8b\" stroke-width=\"1.5\" marker-end=\"url(#dependency-arrow)\">");
    for TaskDependency {
        predecessor,
        successor,
    } in dependencies
    {
        let (Some(&from_row), Some(&to_row)) =
            (row_indices.get(&predecessor), row_indices.get(&successor))
        else {
            continue;
        };
        let from = &rows[from_row];
        let to = &rows[to_row];
        let (Some(finish), Some(start)) = (from.finish_date, to.start_date) else {
            continue;
        };
        let x1 = LABEL_COLUMN_WIDTH + (bounds.start.days_until(finish) as f32 + 1.0) * DAY_WIDTH;
        let x2 = LABEL_COLUMN_WIDTH + bounds.start.days_until(start) as f32 * DAY_WIDTH;
        let y1 = HEADER_HEIGHT + from_row as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.0;
        let y2 = HEADER_HEIGHT + to_row as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.0;
        let elbow = (x1 + x2) / 2.0;
        svg.push_str(&format!(
            "<path d=\"M{x1:.1},{y1:.1} H{elbow:.1} V{y2:.1} H{x2:.1}\"/>"
        ));
    }
    svg.push_str("</g>");
}

fn dependencies_by_successor(sheet: &thinoda_core::Sheet) -> BTreeMap<TopicId, Vec<TopicId>> {
    let mut dependencies = BTreeMap::<TopicId, Vec<TopicId>>::new();
    for TaskDependency {
        predecessor,
        successor,
    } in &sheet.scheduling.dependencies
    {
        dependencies
            .entry(*successor)
            .or_default()
            .push(*predecessor);
    }
    for predecessors in dependencies.values_mut() {
        predecessors.sort_unstable();
    }
    dependencies
}

fn xml_escape(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            '\u{9}'
            | '\u{A}'
            | '\u{D}'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}' => character.to_string(),
            _ => "\u{FFFD}".into(),
        })
        .collect()
}

fn append_csv_field(output: &mut String, value: &str) -> Result<(), IoError> {
    let quoted = value.contains([',', '"', '\r', '\n']);
    let escaped_quotes = value.bytes().filter(|byte| *byte == b'"').count();
    let field_len = value
        .len()
        .checked_add(escaped_quotes)
        .and_then(|len| len.checked_add(if quoted { 2 } else { 0 }))
        .ok_or(IoError::GanttLimit("CSV size"))?;
    ensure_csv_capacity(output, field_len)?;
    if quoted {
        output.push('"');
    }
    for character in value.chars() {
        output.push(character);
        if character == '"' {
            output.push('"');
        }
    }
    if quoted {
        output.push('"');
    }
    Ok(())
}

fn append_csv_dependencies(
    output: &mut String,
    dependencies: &BTreeMap<TopicId, Vec<TopicId>>,
    topic: TopicId,
    sheet: &Sheet,
) -> Result<(), IoError> {
    let Some(predecessors) = dependencies.get(&topic) else {
        return Ok(());
    };
    let mut value_len = 0usize;
    let mut escaped_quotes = 0usize;
    let mut quoted = false;
    for (index, predecessor) in predecessors.iter().enumerate() {
        if index > 0 {
            value_len = value_len
                .checked_add(2)
                .ok_or(IoError::GanttLimit("CSV size"))?;
        }
        let title = &sheet.topics[predecessor].title;
        value_len = value_len
            .checked_add(title.len())
            .ok_or(IoError::GanttLimit("CSV size"))?;
        escaped_quotes = escaped_quotes
            .checked_add(title.bytes().filter(|byte| *byte == b'"').count())
            .ok_or(IoError::GanttLimit("CSV size"))?;
        quoted |= title.contains([',', '"', '\r', '\n']);
    }
    let field_len = value_len
        .checked_add(escaped_quotes)
        .and_then(|len| len.checked_add(if quoted { 2 } else { 0 }))
        .ok_or(IoError::GanttLimit("CSV size"))?;
    ensure_csv_capacity(output, field_len)?;
    if quoted {
        output.push('"');
    }
    for (index, predecessor) in predecessors.iter().enumerate() {
        if index > 0 {
            output.push_str("; ");
        }
        for character in sheet.topics[predecessor].title.chars() {
            output.push(character);
            if character == '"' {
                output.push('"');
            }
        }
    }
    if quoted {
        output.push('"');
    }
    Ok(())
}

fn append_csv_text(output: &mut String, text: &str) -> Result<(), IoError> {
    ensure_csv_capacity(output, text.len())?;
    output.push_str(text);
    Ok(())
}

fn ensure_csv_capacity(output: &str, additional: usize) -> Result<(), IoError> {
    if output
        .len()
        .checked_add(additional)
        .is_none_or(|size| size > MAX_GANTT_CSV_BYTES)
    {
        return Err(IoError::GanttLimit("CSV size"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::{Task, WorkCalendar};

    fn date(value: &str) -> LocalDate {
        value.parse().unwrap()
    }

    fn sample_sheet() -> (Sheet, TopicId, TopicId, TopicId) {
        let mut sheet = Sheet::new("Schedule", "Root");
        let first = sheet.add_child(sheet.root, None, "Plan, <&> \"A\"");
        let second = sheet.add_child(sheet.root, None, "Build 日本語");
        let unscheduled = sheet.add_child(sheet.root, None, "Unscheduled");
        sheet.scheduling.tasks.insert(
            first,
            Task {
                start_date: Some(date("2024-02-28")),
                duration_days: Some(2),
                progress_percent: 50,
                priority: None,
            },
        );
        sheet.scheduling.tasks.insert(
            second,
            Task {
                start_date: None,
                duration_days: None,
                progress_percent: 0,
                priority: None,
            },
        );
        sheet.scheduling.tasks.insert(unscheduled, Task::default());
        sheet.scheduling.dependencies.push(TaskDependency {
            predecessor: first,
            successor: second,
        });
        (sheet, first, second, unscheduled)
    }

    #[test]
    fn chart_has_exact_bounds_stable_rows_and_escaped_dependency_connector() {
        let (sheet, first, second, unscheduled) = sample_sheet();
        let chart = render_gantt(&sheet).unwrap();
        assert_eq!(
            chart.bounds,
            Some(GanttBounds {
                start: date("2024-02-28"),
                finish: date("2024-03-01"),
            })
        );
        assert_eq!(
            chart.rows.iter().map(|row| row.topic).collect::<Vec<_>>(),
            vec![first, second, unscheduled]
        );
        assert_eq!(chart.rows[0].finish_date, Some(date("2024-02-29")));
        assert_eq!(chart.rows[1].start_date, Some(date("2024-03-01")));
        assert_eq!(chart.rows[2].start_date, None);
        assert!(chart.svg.contains("Plan, &lt;&amp;&gt; &quot;A&quot;"));
        assert!(chart.svg.contains("Build 日本語"));
        assert!(chart.svg.contains("marker-end=\"url(#dependency-arrow)\""));
        assert!(chart.svg.contains("2024-02-28 – 2024-03-01"));
        assert!(chart.svg.contains("x1=\"548.0\""));
        assert!(chart.svg.contains("M548.0,87.0"));
    }

    #[test]
    fn all_undated_tasks_are_kept_and_csv_uses_stable_quoted_rows() {
        let (mut sheet, first, second, unscheduled) = sample_sheet();
        let scheduled_csv = String::from_utf8(export_gantt_csv(&sheet).unwrap()).unwrap();
        assert!(scheduled_csv.contains("Build 日本語,2024-03-01,,,0,\"Plan, <&> \"\"A\"\"\""));
        for task in sheet.scheduling.tasks.values_mut() {
            task.start_date = None;
            task.duration_days = None;
        }
        sheet.scheduling.dependencies.clear();
        let chart = render_gantt(&sheet).unwrap();
        assert_eq!(chart.bounds, None);
        assert!(chart.svg.contains("No scheduled dates"));
        assert_eq!(chart.rows.len(), 3);

        let csv = String::from_utf8(export_gantt_csv(&sheet).unwrap()).unwrap();
        assert!(
            csv.starts_with("Task,Start,Finish,Duration (working days),Progress,Dependencies\r\n")
        );
        assert!(csv.contains("\"Plan, <&> \"\"A\"\"\",,,,50,"));
        let rows = chart.rows.iter().map(|row| row.topic).collect::<Vec<_>>();
        assert_eq!(rows, vec![first, second, unscheduled]);
    }

    #[test]
    fn invalid_and_cyclic_schedules_return_core_errors() {
        let (mut sheet, first, second, _) = sample_sheet();
        sheet.scheduling.dependencies.push(TaskDependency {
            predecessor: second,
            successor: first,
        });
        assert!(matches!(
            render_gantt(&sheet),
            Err(IoError::GanttSchedule(
                thinoda_core::ScheduleError::DependencyCycle
            ))
        ));

        sheet.scheduling.dependencies.clear();
        sheet
            .scheduling
            .tasks
            .get_mut(&first)
            .unwrap()
            .progress_percent = 101;
        assert!(matches!(
            render_gantt(&sheet),
            Err(IoError::GanttSchedule(
                thinoda_core::ScheduleError::InvalidProgress(101)
            ))
        ));
    }

    #[test]
    fn control_characters_are_replaced_and_empty_task_tables_fail() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let topic = sheet.add_child(sheet.root, None, "Bad\u{1} title");
        sheet.scheduling.tasks.insert(topic, Task::default());
        let chart = render_gantt(&sheet).unwrap();
        assert!(chart.svg.contains("Bad\u{FFFD} title"));
        assert!(!chart.svg.contains('\u{1}'));

        sheet.scheduling.tasks.clear();
        assert!(matches!(render_gantt(&sheet), Err(IoError::GanttNoTasks)));
    }

    #[test]
    fn timeline_limit_is_checked_and_exact_supported_range_is_included() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let first = sheet.add_child(sheet.root, None, "First");
        let last = sheet.add_child(sheet.root, None, "Last");
        let start = date("2024-01-01");
        let supported_end = start
            .checked_add_days((MAX_GANTT_TIMELINE_DAYS - 1) as i32)
            .unwrap();
        sheet.scheduling.tasks.insert(
            first,
            Task {
                start_date: Some(start),
                ..Task::default()
            },
        );
        sheet.scheduling.tasks.insert(
            last,
            Task {
                start_date: Some(supported_end),
                ..Task::default()
            },
        );
        let chart = render_gantt(&sheet).unwrap();
        assert_eq!(chart.bounds.unwrap().finish, supported_end);

        let outside = supported_end.checked_add_days(1).unwrap();
        sheet.scheduling.tasks.get_mut(&last).unwrap().start_date = Some(outside);
        assert!(matches!(
            render_gantt(&sheet),
            Err(IoError::GanttLimit("timeline length"))
        ));
        assert!(export_gantt_csv(&sheet).is_ok());
    }

    #[test]
    fn image_pdf_and_print_outputs_reuse_rendering_helpers() {
        let (sheet, _, _, _) = sample_sheet();
        let chart = render_gantt(&sheet).unwrap();
        let png = chart.png(1.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pdf = chart.pdf().unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
        let print = chart.print(PrintSettings::default()).unwrap();
        assert_eq!(print.image.dimensions(), (1754, 1240));
        assert!(print.pdf().unwrap().starts_with(b"%PDF-"));
        assert!(chart.png(0.0).is_err());
    }

    #[test]
    fn png_image_side_and_pixel_limits_are_checked_before_rasterization() {
        let side_limited = GanttDocument {
            svg: String::new(),
            bounds: None,
            rows: Vec::new(),
            dimensions: [MAX_GANTT_IMAGE_SIDE as f32 + 1.0, 1.0],
        };
        assert!(matches!(
            side_limited.png(1.0),
            Err(IoError::GanttLimit("PNG image side"))
        ));
        let pixel_limited = GanttDocument {
            dimensions: [8_000.0, 5_000.0],
            ..side_limited
        };
        assert!(matches!(
            pixel_limited.png(1.0),
            Err(IoError::GanttLimit("PNG pixel count"))
        ));
    }

    #[test]
    fn calendar_exception_affects_rendered_inclusive_working_duration() {
        let (mut sheet, first, _, _) = sample_sheet();
        sheet.scheduling.calendar = WorkCalendar::default();
        sheet
            .scheduling
            .calendar
            .exceptions
            .insert(date("2024-02-29"), false);
        sheet
            .scheduling
            .tasks
            .get_mut(&first)
            .unwrap()
            .duration_days = Some(1);
        let chart = render_gantt(&sheet).unwrap();
        assert_eq!(chart.rows[0].finish_date, Some(date("2024-02-28")));
    }

    #[test]
    fn label_and_svg_size_limits_are_reported_before_returning_large_documents() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let topic = sheet.add_child(sheet.root, None, "x".repeat(MAX_GANTT_LABEL_BYTES + 1));
        sheet.scheduling.tasks.insert(topic, Task::default());
        assert!(matches!(
            render_gantt(&sheet),
            Err(IoError::GanttLimit("label size"))
        ));

        sheet.topics.get_mut(&topic).unwrap().title = "&".repeat(3_400_000);
        assert!(matches!(
            render_gantt(&sheet),
            Err(IoError::GanttLimit("SVG size"))
        ));
    }

    #[test]
    fn csv_size_limit_covers_repeated_dependency_labels() {
        let mut sheet = Sheet::new("Schedule", "Root");
        let predecessor = sheet.add_child(sheet.root, None, "x".repeat(3_400_000));
        sheet.scheduling.tasks.insert(
            predecessor,
            Task {
                start_date: Some(date("2024-01-01")),
                duration_days: Some(1),
                ..Task::default()
            },
        );
        for index in 0..5 {
            let successor = sheet.add_child(sheet.root, None, format!("Successor {index}"));
            sheet.scheduling.tasks.insert(successor, Task::default());
            sheet.scheduling.dependencies.push(TaskDependency {
                predecessor,
                successor,
            });
        }
        assert!(matches!(
            export_gantt_csv(&sheet),
            Err(IoError::GanttLimit("CSV size"))
        ));
    }
}
