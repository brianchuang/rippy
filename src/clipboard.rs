use std::process::Command;

/// Read the current clipboard contents and the change count.
/// Returns (content, change_count).
pub fn get_clipboard() -> (Option<String>, i64) {
    // Use pbpaste for content
    let content = Command::new("pbpaste")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty());

    // Use osascript to get the change count from NSPasteboard
    let change_count = Command::new("osascript")
        .args([
            "-e",
            "use framework \"AppKit\"",
            "-e",
            "set pb to current application's NSPasteboard's generalPasteboard()",
            "-e",
            "pb's changeCount() as integer",
        ])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<i64>().ok())
            } else {
                None
            }
        })
        .unwrap_or(0);

    (content, change_count)
}

/// Set the clipboard to the given string.
pub fn set_clipboard(content: &str) {
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to run pbcopy");
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes()).ok();
    }
    child.wait().ok();
}
