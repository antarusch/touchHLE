/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Logging and terminal output macros.

use std::fs::File;
use std::sync::LazyLock;

/// Get a handle to the log file. This is only for use by logging macros!
///
/// All the logging macros print to stderr or (on Android) logcat, but this
/// is not convenient for users who aren't accustomed to command-line tools or
/// who don't have access to ADB, so we also write to a log file.
pub fn get_log_file() -> &'static File {
    static LOG_FILE: LazyLock<File> = LazyLock::new(|| {
        File::create(crate::paths::user_data_base_path().join("touchHLE_log.txt")).unwrap()
    });

    &LOG_FILE
}

/// Prints a log message unconditionally. Use this for errors or warnings.
///
/// The message is prefixed with the module path, so it is clear where it comes
/// from.
macro_rules! log {
    ($($arg:tt)+) => {
        echo!("{}: {}", module_path!(), format_args!($($arg)+));
    }
}

/// Same as [log], but silently fails on panic instead of
/// panicking.
macro_rules! log_no_panic {
    ($($arg:tt)+) => {
        echo_no_panic!("{}: {}", module_path!(), format_args!($($arg)+));
    }
}

/// Like [log], but prints the message only if debugging is enabled for the
/// module where it is used. This can be used for verbose things only needed
/// when debugging.
macro_rules! log_dbg {
    ($($arg:tt)+) => {
        if $crate::log::diagnostic_log_enabled(module_path!()) {
            log!($($arg)*);
        }
    }
}

/// Like [log], but messages only log once and cannot have formatting.
/// To be used for log messages that are known to spam the log file (like those
/// logged every frame).
macro_rules! log_once {
    ($msg:literal) => {{
        static LOG_ONCE: std::sync::Once = std::sync::Once::new();
        LOG_ONCE.call_once(|| {
            log!("{} [this log will only be shown once]", $msg);
        });
    }};
}

/// Print a message (with implicit newline). This should be used for all
/// touchHLE output that isn't coming from the app itself.
///
/// Prefer use [log] or [log_dbg] for errors and warnings during emulation.
macro_rules! echo {
    ($($arg:tt)+) => {
        {
            let formatted_str = format!($($arg)+);

            #[cfg(target_os = "android")]
            {
                sdl2::log::log(&formatted_str);
            }
            #[cfg(not(target_os = "android"))]
            eprintln!("{}", formatted_str);

            use std::io::Write;
            let mut log_file = $crate::log::get_log_file();
            let _ = log_file.write_all(formatted_str.as_bytes());
            let _ = log_file.write_all(b"\n");
        }
    };
    () => {
        {
            #[cfg(target_os = "android")]
            {
                sdl2::log::log("");
            }
            #[cfg(not(target_os = "android"))]
            eprintln!("");

            use std::io::Write;
            let _ = $crate::log::get_log_file().write_all(b"\n");
        }
    }
}

/// Same as [echo], but silently fails on panic instead of
/// panicking.
macro_rules! echo_no_panic {
    ($($arg:tt)*) => {
        {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                echo!($($arg)*);
            }));
        }
    }
}

/// Put modules to enable [log_dbg] for here, e.g. "touchHLE::mem" to see when
/// memory is allocated and freed.
pub const ENABLED_MODULES: &[&str] = &[
    "touchHLE::frameworks::uikit::ui_application",
    "touchHLE::frameworks::uikit::ui_view::ui_window",
    "touchHLE::frameworks::uikit::ui_view_controller",
    "touchHLE::frameworks::foundation::ns_lock",
    "touchHLE::frameworks::foundation::ns_thread",
    "touchHLE::libc::pthread::cond",
];

/// Bound the verbose trace so a guest spin loop cannot fill storage.
pub fn diagnostic_log_enabled(module: &str) -> bool {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static MESSAGES: AtomicUsize = AtomicUsize::new(0);
    if !ENABLED_MODULES.contains(&module) {
        return false;
    }
    let count = MESSAGES.fetch_add(1, Ordering::Relaxed);
    if count == 4000 {
        echo!("Diagnostic API trace limit reached; periodic freeze reports continue.");
    }
    count < 4000
}

type HostCallStacks = std::collections::HashMap<usize, Vec<String>>;
static HOST_CALLS: LazyLock<std::sync::Mutex<HostCallStacks>> = LazyLock::new(Default::default);

/// Track host calls even when a call never yields back to the scheduler.
pub struct DiagnosticHostCall {
    thread: usize,
}

impl DiagnosticHostCall {
    pub fn enter(thread: usize, description: String) -> Self {
        HOST_CALLS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(thread)
            .or_default()
            .push(description);
        Self { thread }
    }
}

impl Drop for DiagnosticHostCall {
    fn drop(&mut self) {
        if let Some(stack) = HOST_CALLS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&self.thread)
        {
            stack.pop();
        }
    }
}

fn active_host_calls() -> String {
    use std::fmt::Write;
    let stacks = HOST_CALLS.lock().unwrap_or_else(|error| error.into_inner());
    let mut text = String::new();
    for (thread, stack) in stacks.iter().filter(|(_, stack)| !stack.is_empty()) {
        let _ = writeln!(text, "Active host calls for thread #{thread}: {stack:?}");
    }
    text
}

/// Captures guest state on the emulator thread. Only owned text is sent to the
/// independent writer: it never accesses the emulator or guest memory.
pub struct FreezeReporter {
    sender: std::sync::mpsc::SyncSender<(std::time::Instant, String)>,
    next_capture: std::time::Instant,
}

impl FreezeReporter {
    pub fn start() -> Option<Self> {
        use std::io::Write;
        use std::sync::mpsc::{sync_channel, RecvTimeoutError};
        use std::time::{Duration, Instant};

        let path = crate::paths::user_data_base_path().join("touchHLE_freeze_report.txt");
        let mut file = match File::create(&path) {
            Ok(file) => file,
            Err(error) => {
                log!("Could not create freeze report: {error}");
                return None;
            }
        };
        let _ = writeln!(file, "Eustrath freeze diagnostic: {}", crate::VERSION);
        let _ = writeln!(
            file,
            "Samples record thread state, not gameplay progress.\n"
        );
        let _ = file.sync_data();
        let (sender, receiver) = sync_channel::<(Instant, String)>(1);
        let started = Instant::now();
        let result = std::thread::Builder::new()
            .name("freeze-report".into())
            .spawn(move || {
                let mut latest = (
                    started,
                    String::from("Waiting for first scheduler checkpoint"),
                );
                let mut next_report = started;
                // Limit recording to ten minutes. Normal logs remain enabled.
                for _ in 0..120 {
                    loop {
                        let wait = next_report.saturating_duration_since(Instant::now());
                        match receiver.recv_timeout(wait) {
                            Ok(snapshot) => latest = snapshot,
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => {
                                let _ = writeln!(file, "Emulator session ended.\n{}", latest.1);
                                let _ = file.sync_data();
                                return;
                            }
                        }
                    }
                    let _ = writeln!(
                        file,
                        "Elapsed {:.1}s; checkpoint age {:.1}s\n{}\n{}\n",
                        started.elapsed().as_secs_f32(),
                        latest.0.elapsed().as_secs_f32(),
                        latest.1,
                        active_host_calls()
                    );
                    let _ = file.sync_data();
                    next_report = Instant::now() + Duration::from_secs(5);
                }
                let _ = writeln!(file, "Ten-minute capture limit reached.");
                let _ = file.sync_data();
            });
        if let Err(error) = result {
            log!("Could not start freeze report writer: {error}");
            return None;
        }
        log!("Freeze diagnostics enabled: {}", path.display());
        Some(Self {
            sender,
            next_capture: started,
        })
    }

    pub fn capture_due(&self) -> bool {
        std::time::Instant::now() >= self.next_capture
    }

    pub fn capture(&mut self, snapshot: String) {
        let now = std::time::Instant::now();
        // A full channel must not block guest execution.
        let _ = self.sender.try_send((now, snapshot));
        self.next_capture = now + std::time::Duration::from_secs(1);
    }
}
