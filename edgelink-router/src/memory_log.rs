use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_MEMORY_LOGS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Normal,
    Wait,
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level_label: &'static str,
    pub level_color: u32,
    pub badge_bg: u32,
    pub badge_border: u32,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct MemoryLogBuffer {
    entries: VecDeque<LogEntry>,
}

impl MemoryLogBuffer {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_MEMORY_LOGS),
        }
    }

    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        let (level_label, level_color, badge_bg, badge_border) = match level {
            LogLevel::Normal => ("正常", 0x4ade80, 0x052e16, 0x14532d),
            LogLevel::Wait => ("等待", 0xfacc15, 0x271705, 0x713f12),
            LogLevel::Info => ("状态", 0xd4d4d8, 0x18181b, 0x3f3f46),
            LogLevel::Error => ("异常", 0xfca5a5, 0x250c0c, 0x7f1d1d),
        };
        let entry = LogEntry {
            timestamp: current_timestamp_string(),
            level_label,
            level_color,
            badge_bg,
            badge_border,
            message: message.into(),
        };
        if self.entries.len() >= MAX_MEMORY_LOGS {
            self.entries.pop_back();
        }
        self.entries.push_front(entry);
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }
}

fn current_timestamp_string() -> String {
    let epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_unix_timestamp(epoch_secs)
}

fn format_unix_timestamp(epoch_secs: u64) -> String {
    let local_secs = epoch_secs.saturating_add(8 * 3600);
    let days = (local_secs / 86_400) as i64;
    let time_of_day = local_secs % 86_400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02} {hour:02}:{minute:02}:{second:02}")
}
