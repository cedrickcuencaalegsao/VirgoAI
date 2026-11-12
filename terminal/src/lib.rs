//! Run a shell command in the project folder and capture its output.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_OUTPUT: usize = 20_000;

#[derive(Debug, Clone)]
pub struct RunResult {
    pub output: String,
    pub code: Option<i32>,
    pub timed_out: bool,
}

/// Last `max` characters (errors are usually at the end).
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let skipped: String = text.chars().skip(count - max).collect();
    format!("...{skipped}")
}

pub async fn run(root: PathBuf, command: String) -> RunResult {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    // Login shell so ~/.cargo/bin etc. are on PATH even when launched from Finder.
    // The newlines keep a trailing `# comment` from swallowing the `)`.
    let wrapped = format!("(\n{command}\n) 2>&1");

    let mut cmd = Command::new(shell);
    cmd.arg("-lc")
        .arg(wrapped)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .env("CARGO_TERM_COLOR", "never")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => RunResult {
            output: tail(&String::from_utf8_lossy(&out.stdout), MAX_OUTPUT),
            code: out.status.code(),
            timed_out: false,
        },
        Ok(Err(e)) => RunResult { output: format!("failed to start: {e}"), code: None, timed_out: false },
        Err(_) => RunResult {
            output: format!("(stopped after {}s)", TIMEOUT.as_secs()),
            code: None,
            timed_out: true,
        },
    }
}