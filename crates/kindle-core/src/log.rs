//! Kindle's log: `kindle\logs\kindle.log` (previous run kept as `kindle.prev.log`) plus an
//! in-memory ring buffer the console can show with `:log`.

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            _ => return None,
        })
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
        }
    }
}

struct Logger {
    file: Option<File>,
    recent: VecDeque<String>,
    min: Level,
    start: Instant,
}

const RECENT_LINES: usize = 300;

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);
/// When the log started, readable without the logger's lock (crash.rs).
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Seconds since the log started, as its lines show them.
pub fn elapsed() -> f64 {
    START.get().map_or(0.0, |s| s.elapsed().as_secs_f64())
}

pub fn init(logs_dir: &Path) {
    let _ = fs::create_dir_all(logs_dir);
    let current = logs_dir.join("kindle.log");
    let _ = fs::rename(&current, logs_dir.join("kindle.prev.log"));
    let file = File::create(&current).ok();
    let start = *START.get_or_init(Instant::now);
    *LOGGER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Logger { file, recent: VecDeque::with_capacity(RECENT_LINES), min: Level::Info, start });
}

pub fn set_level(level: Level) {
    if let Some(l) = LOGGER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        l.min = level;
    }
}

pub fn write(level: Level, source: &str, msg: &str) {
    let mut guard = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
    let Some(l) = guard.as_mut() else { return };
    if level < l.min {
        return;
    }
    let t = l.start.elapsed().as_secs_f64();
    let line = format!("[{t:9.3}] {} [{source}] {msg}", level.tag());
    if let Some(f) = l.file.as_mut() {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
    if l.recent.len() == RECENT_LINES {
        l.recent.pop_front();
    }
    l.recent.push_back(line);
}

/// The last `n` log lines, oldest first.
pub fn recent(n: usize) -> Vec<String> {
    let guard = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|l| l.recent.iter().rev().take(n).rev().cloned().collect()).unwrap_or_default()
}

#[macro_export]
macro_rules! info { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Info, "kindle", &format!($($a)*)) } }
#[macro_export]
macro_rules! warn { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Warn, "kindle", &format!($($a)*)) } }
#[macro_export]
macro_rules! error { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Error, "kindle", &format!($($a)*)) } }
#[macro_export]
macro_rules! debug { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Debug, "kindle", &format!($($a)*)) } }
