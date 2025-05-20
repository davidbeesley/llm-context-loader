use log::LevelFilter::Debug;
use log::{Level, LevelFilter, Record, info, log_enabled};
use std::env;
use std::fmt::{Arguments, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, process};

use ansi_colors::ColouredStr;
use chrono::{DateTime, Utc};
use env_logger::Target;
use lazy_static::lazy_static;
use nohash_hasher::NoHashHasher;
use serde::Serialize;
use smallstr::SmallString;
use std::fmt::Write as FmtWrite;
use std::path::Path;
use std::time::SystemTime;
use stdio_override::{StderrOverride, StderrOverrideGuard, StdoutOverride, StdoutOverrideGuard};

pub fn debug() {
    CoreLogger::init_with_filter(Debug);
}

pub fn info() {
    CoreLogger::init_with_filter(log::LevelFilter::Info);
}

static START: Once = Once::new();

pub struct CoreLogger;

/// list of modules to force logging at the Warn level no matter what
/// TODO MAKE THIS A CONFIG
const THIRD_PARTY_MODULES_TO_FILTER: [&str; 10] = [
    "aws_config",
    "aws_smithy_http",
    "hyper",
    "isahc",
    "mio",
    "polling",
    "rafx",
    "reqwest",
    "rustls",
    "tungstenite",
];
// const MAX_CRATE_NAME_LEN: usize = 15;
const MAX_FILENAME_LEN: usize = 25;
const PADDING: &str = "                                                                                                                                      ";
// 134 spaces
/// Default maximum line width for log messages, overwritten by terminal width if available
const MAX_MESSAGE_LINE_WIDTH: usize = 120;
const MIN_MESSAGE_LINE_WIDTH: usize = 30;
const PREFIX_LEN: usize = MAX_FILENAME_LEN + 54;

const WRAPPED: &str = "    ->    ";

impl CoreLogger {
    pub fn init() {
        CoreLogger::init_with_filter(Debug);
    }
    pub fn init_with_filter(level: LevelFilter) {
        START.call_once(|| {
            // Filtering here doesn't improve performance while filtering in the process.toml file does.
            let mut builder = env_logger::builder();

            let last_log_hash = AtomicU64::new(0);
            // this is one past the actual number of trace logs that have repeated,
            // so when a repeat happens, we'll be processing the second occurrence
            let repeat_count = AtomicU64::new(2);
            let next_repeat_count_to_print_at = AtomicU64::new(2);

            builder
                .filter_level(level)
                .format(move |buf, record| {
                    let record = CoreLoggerRecord::from_record(record);
                    if log_enabled!(Level::Trace) {
                        let mut hasher = ahash::AHasher::default();
                        record.module_path.hash(&mut hasher);
                        record.file.hash(&mut hasher);
                        record.line.hash(&mut hasher);
                        record.tid.hash(&mut hasher);
                        let mut repeat_message_check: SmallString<[u8; 256]> = SmallString::new();
                        write!(&mut repeat_message_check, "{}", record.message)
                            .expect("stringify log message");
                        repeat_message_check.hash(&mut hasher);
                        let log_hash = hasher.finish();
                        let last_log_hash = last_log_hash.swap(log_hash, Ordering::AcqRel);
                        if log_hash == last_log_hash {
                            let reps = repeat_count.fetch_add(1, Ordering::AcqRel);
                            if reps == next_repeat_count_to_print_at.load(Ordering::Acquire) {
                                next_repeat_count_to_print_at.store(reps * reps, Ordering::Release);
                                writeln!(buf, "{} ({})", record, reps)
                            } else {
                                Ok(())
                            }
                        } else {
                            repeat_count.store(2, Ordering::Release);
                            next_repeat_count_to_print_at.store(2, Ordering::Release);
                            writeln!(buf, "{}", record)
                        }
                    } else {
                        writeln!(buf, "{}", record)
                    }
                })
                .target(Target::Stdout);

            for module in THIRD_PARTY_MODULES_TO_FILTER {
                builder.filter_module(module, LevelFilter::Warn);
            }

            //TODO: Rather than disabling this, maybe we should be using it?
            tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default())
                .expect("disable tracing");

            builder.init();
            info!("Starting logger for process with pid: {}", process::id());
        })
    }
}

#[derive(Serialize, Debug)]
pub struct CoreLoggerRecord<'a> {
    level: Level,
    target: &'a str,
    pid: u32,

    tid: u64,
    module_path: &'a str,
    file: &'a str,
    line: u32,
    time: SystemTime,
    message: &'a Arguments<'a>,
}

impl<'a> CoreLoggerRecord<'a> {
    pub fn from_record(record: &'a Record) -> CoreLoggerRecord<'a> {
        lazy_static! {
            static ref PID: u32 = process::id();
        }

        //HACK: getting the u64 thread ID using .as_u64() is still unstable, but ThreadId
        //implements Hash, so we can use the NoHashHasher to get the u64 out safely.
        let mut hasher: NoHashHasher<u32> = NoHashHasher::default();
        std::thread::current().id().hash(&mut hasher);
        let tid = hasher.finish();

        Self {
            level: record.level(),
            target: record.target(),
            pid: *PID,
            tid,
            module_path: record.module_path().unwrap_or("None"),
            file: record.file().unwrap_or("None"),
            line: record.line().unwrap_or(0),
            time: SystemTime::now(),
            message: record.args(),
        }
    }

    fn format_level_color(level: Level) -> ColouredStr<'static> {
        match level {
            Level::Error => {
                let mut s = ColouredStr::new("ERROR");
                s.red();
                s
            }
            Level::Warn => {
                let mut s = ColouredStr::new(" WARN");
                s.yellow();
                s
            }
            Level::Info => {
                let mut s = ColouredStr::new(" INFO");
                s.green();
                s
            }
            Level::Debug => {
                let mut s = ColouredStr::new("DEBUG");
                s.blue();
                s
            }
            Level::Trace => {
                let mut s = ColouredStr::new("TRACE");
                s.cyan();
                s
            }
        }
    }
}

impl Display for CoreLoggerRecord<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let datetime: DateTime<Utc> = self.time.into();
        let time_stamp = datetime.format("[%b %d, %Y  %T %.3f]");
        let level_str = CoreLoggerRecord::format_level_color(self.level);

        let tid = self.tid;
        let line = self.line;
        // let mut crate_name = self
        //     .module_path
        //     .split("::")
        //     .next()
        //     .unwrap_or("Crate_Unknown");
        // if crate_name.len() > MAX_CRATE_NAME_LEN {
        //     crate_name = &crate_name[0..MAX_CRATE_NAME_LEN];
        // }
        let mut filename = self.file.split('/').last().unwrap_or("File_unknown");
        if filename.len() > MAX_FILENAME_LEN {
            filename = &filename[0..MAX_FILENAME_LEN];
        }
        let padding_len = MAX_FILENAME_LEN - filename.len() + 2;
        let padding = &PADDING[0..padding_len];

        // Write header
        write!(
            f,
            "{level_str:5}  {time_stamp}  [{tid:0>3}]  [{filename}:{line:<4}]{padding}" // TODO "  "
        )?;

        // Write message
        let mut message: SmallString<[u8; 256]> = SmallString::new();
        write!(&mut message, "{}", self.message)?;

        let mut remaining_message = message.as_str();

        while let Some((prefix, suffix)) = remaining_message.split_once('\n') {
            write_single_line(f, prefix)?;
            let padding = &PADDING[0..PREFIX_LEN];
            write!(f, "\n{}", padding)?;
            remaining_message = suffix;
        }
        write_single_line(f, remaining_message)
    }
}

// --- copied from nightly std. when these are stablized we won't need them any more
#[inline]
const fn is_utf8_char_boundary(x: u8) -> bool {
    // This is bit magic equivalent to: b < 128 || b >= 192
    (x as i8) >= -0x40
}

pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        s.len()
    } else {
        let lower_bound = index.saturating_sub(3);
        let new_index = s.as_bytes()[lower_bound..=index]
            .iter()
            .cloned()
            .rposition(is_utf8_char_boundary);

        // SAFETY: we know that the character boundary will be within four bytes
        unsafe { lower_bound + new_index.unwrap_unchecked() }
    }
}
// ---
//

#[inline]
fn get_max_line_width() -> usize {
    match termion::terminal_size() {
        Ok((w, _)) => (w as usize - PREFIX_LEN - 1).max(MIN_MESSAGE_LINE_WIDTH),
        Err(_) => MAX_MESSAGE_LINE_WIDTH,
    }
}

#[inline]
fn write_single_line(f: &mut Formatter, line: &str) -> core::fmt::Result {
    let mut line = line;
    let max_width = get_max_line_width();
    let mut width = max_width;
    while line.len() > width {
        width = floor_char_boundary(line, width);
        let write_now = &line[0..width];

        writeln!(f, "{}", write_now)?;
        line = &line[width..];
        let padding = &PADDING[0..PREFIX_LEN];
        write!(f, "{}{}", padding, WRAPPED)?;

        width = max_width - WRAPPED.len();
    }
    write!(f, "{}", line)
}

pub fn init_log_file() -> (StdoutOverrideGuard, StderrOverrideGuard) {
    let start_time: DateTime<Utc> = SystemTime::now().into();
    let start_time = start_time.format("%Y_%m_%d__%T%.3fz");
    let cwd = env::current_dir()
        .expect("Failed to get current dir")
        .file_name()
        .expect("Should not fail")
        .to_str()
        .expect("Convert failed")
        .to_string();
    let mut log_dir = dirs::home_dir().expect("Home directory not found");
    log_dir.push(".core_logs");
    log_dir.push(Path::new(&cwd));
    let executable_path = std::env::current_exe().expect("get path to current executable");
    log_dir.push(
        Path::new(&executable_path)
            .file_name()
            .expect("Should not fail"),
    );
    let mut log_file = log_dir.clone();
    log_file.push(format!("{}.txt", start_time));
    let mut sym_link = log_dir.clone();
    sym_link.push("last.txt");
    println!("Logging to {}", log_file.display());
    fs::create_dir_all(log_dir).expect("Failed to ensure log dir");

    let stdout_guard =
        StdoutOverride::override_file(log_file.clone()).expect("Failed to override stdout");
    let stderr_guard =
        StderrOverride::override_file(log_file.clone()).expect("Failed to override stderr");
    println!("Begin log {}", log_file.display());
    match std::os::unix::fs::symlink(log_file.clone(), sym_link.clone()) {
        Ok(_) => {
            println!(
                "Updated symlink {} -> {}",
                sym_link.display(),
                log_file.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            match fs::remove_file(sym_link.clone()) {
                Ok(_) => {
                    std::os::unix::fs::symlink(log_file.clone(), sym_link.clone())
                        .expect("Unable to update symlink");
                    println!(
                        "Updated symlink {} -> {}",
                        sym_link.display(),
                        log_file.display()
                    );
                }
                Err(_) => {
                    println!(
                        "Unable to update {}. Check if open elsewhere",
                        sym_link.display()
                    )
                }
            }
        }
        Err(e) => {
            println!("Unable to update {} due to {e:?}", sym_link.display())
        }
    }
    (stdout_guard, stderr_guard)
}

#[cfg(test)]
mod tests {
    use log::{debug, error, info, trace, warn};

    use super::CoreLogger;

    #[test]
    fn test_logger() {
        println!("Testing logger");
        CoreLogger::init_with_filter(log::LevelFilter::Trace);
        info!("Hello world");
        info!("Hello world\nHello world");
        trace!("Trace");
        debug!("Debug");
        info!("Info");
        warn!("Warn");
        error!("Error");
        let long_string = "*".repeat(1000);
        info!("{}_end", long_string);
        info!("{}\n{}\n{}", long_string, long_string, long_string);
        for _ in 0..1000 {
            trace!("repeat");
        }
    }
}
