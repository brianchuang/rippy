use crate::clipboard;
use crate::db::Store;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub fn run(foreground: bool, db_path: &Path, pid_path: &Path) {
    if !foreground {
        // Check if already running
        if let Some(pid) = read_pid(pid_path) {
            if is_running(pid) {
                eprintln!("Daemon already running (PID {pid}). Use `rippy stop` first.");
                std::process::exit(1);
            }
        }

        // Fork to background
        match daemonize::Daemonize::new()
            .pid_file(pid_path)
            .working_directory(".")
            .start()
        {
            Ok(_) => {
                // We're in the child — continue to poll loop
            }
            Err(e) => {
                eprintln!("Failed to daemonize: {e}");
                std::process::exit(1);
            }
        }
    } else {
        // Write PID for foreground mode too
        write_pid(pid_path);
        println!("Rippy daemon running in foreground (PID {})", std::process::id());
        println!("Press Ctrl+C to stop.");
    }

    // Set up signal handler for clean shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, r.clone()).ok();
    signal_hook::flag::register(signal_hook::consts::SIGINT, r).ok();

    poll_loop(db_path, &running);

    // Cleanup
    std::fs::remove_file(pid_path).ok();
}

fn poll_loop(db_path: &Path, running: &AtomicBool) {
    let store = Store::open(db_path).expect("Failed to open database");
    let mut last_change_count: i64 = -1;

    while running.load(Ordering::Relaxed) {
        let (content, change_count) = clipboard::get_clipboard();

        if change_count != last_change_count {
            last_change_count = change_count;
            if let Some(text) = content {
                // Skip very large entries (> 1MB)
                if text.len() <= 1_000_000 {
                    store.insert(&text, None).ok();
                }
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

fn write_pid(path: &Path) {
    std::fs::write(path, std::process::id().to_string()).ok();
}

pub fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn is_running(pid: u32) -> bool {
    // kill with signal 0 checks if the process exists
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
