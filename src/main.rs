mod clipboard;
mod db;
mod daemon;
mod tui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rippy", about = "macOS clipboard history manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the clipboard monitoring daemon
    Daemon {
        /// Run in foreground instead of daemonizing
        #[arg(short, long)]
        foreground: bool,
    },
    /// List recent clipboard entries
    List {
        /// Number of entries to show
        #[arg(short, long, default_value = "20")]
        count: usize,
    },
    /// Search clipboard history
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(short, long, default_value = "20")]
        count: usize,
    },
    /// Copy a history entry back to the clipboard by ID
    Copy {
        /// Entry ID
        id: i64,
    },
    /// Clear all clipboard history
    Clear,
    /// Show daemon status
    Status,
    /// Stop the running daemon
    Stop,
}

fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("rippy");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn db_path() -> PathBuf {
    data_dir().join("history.db")
}

fn pid_path() -> PathBuf {
    data_dir().join("rippy.pid")
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => {
            if let Err(e) = tui::run(&db_path()) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Daemon { foreground }) => {
            daemon::run(foreground, &db_path(), &pid_path());
        }
        Some(Commands::List { count }) => {
            cmd_list(count);
        }
        Some(Commands::Search { query, count }) => {
            cmd_search(&query, count);
        }
        Some(Commands::Copy { id }) => {
            cmd_copy(id);
        }
        Some(Commands::Clear) => {
            cmd_clear();
        }
        Some(Commands::Status) => {
            cmd_status();
        }
        Some(Commands::Stop) => {
            cmd_stop();
        }
    }
}

fn cmd_list(count: usize) {
    let store = db::Store::open(&db_path()).expect("Failed to open database");
    let entries = store.recent(count).expect("Failed to read entries");
    if entries.is_empty() {
        println!("No clipboard history. Start the daemon with: rippy daemon");
        return;
    }
    print_entries(&entries);
}

fn cmd_search(query: &str, count: usize) {
    let store = db::Store::open(&db_path()).expect("Failed to open database");
    let entries = store.search(query, count).expect("Failed to search");
    if entries.is_empty() {
        println!("No matches found.");
        return;
    }
    print_entries(&entries);
}

fn cmd_copy(id: i64) {
    let store = db::Store::open(&db_path()).expect("Failed to open database");
    match store.get(id) {
        Ok(Some(entry)) => {
            clipboard::set_clipboard(&entry.content);
            let preview = truncate(&entry.content, 60);
            println!("Copied to clipboard: {preview}");
        }
        Ok(None) => {
            eprintln!("Entry {id} not found.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_clear() {
    let store = db::Store::open(&db_path()).expect("Failed to open database");
    let count = store.clear().expect("Failed to clear history");
    println!("Cleared {count} entries.");
}

fn cmd_status() {
    match daemon::read_pid(&pid_path()) {
        Some(pid) => {
            if daemon::is_running(pid) {
                println!("Daemon is running (PID {pid})");
            } else {
                println!("Daemon is not running (stale PID file)");
            }
        }
        None => println!("Daemon is not running"),
    }
}

fn cmd_stop() {
    match daemon::read_pid(&pid_path()) {
        Some(pid) => {
            if daemon::is_running(pid) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                println!("Sent stop signal to daemon (PID {pid})");
                std::fs::remove_file(pid_path()).ok();
            } else {
                println!("Daemon is not running (cleaning stale PID file)");
                std::fs::remove_file(pid_path()).ok();
            }
        }
        None => println!("Daemon is not running"),
    }
}

fn print_entries(entries: &[db::ClipEntry]) {
    for entry in entries {
        let preview = truncate(&entry.content, 80);
        let time = entry.timestamp.format("%Y-%m-%d %H:%M:%S");
        println!("{:>5} │ {} │ {}", entry.id, time, preview);
    }
}

fn truncate(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.len() > max {
        format!("{}…", &line[..max])
    } else if s.lines().count() > 1 {
        format!("{line}…")
    } else {
        line.to_string()
    }
}
