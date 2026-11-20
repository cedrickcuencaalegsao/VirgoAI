//! Format source text with the language's standard formatter (stdin -> stdout).
//! If the tool is not installed, falls back to a whitespace cleanup.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct Formatted {
    pub text: String,
    /// "rustfmt", "prettier", ... or "built-in cleanup".
    pub tool: String,
}

enum RunError {
    Missing,
    Failed(String),
}

fn tool_for(root: &Path, rel: &str) -> Option<(String, Vec<String>)> {
    let ext = rel.rsplit_once('.').map(|(_, e)| e)?;
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match ext {
        "rs" => Some(("rustfmt".into(), args(&["--edition", &edition(root, rel)]))),
        "py" => Some(("black".into(), args(&["-q", "-"]))),
        "go" => Some(("gofmt".into(), vec![])),
        "js" | "jsx" | "ts" | "tsx" | "json" | "css" | "html" | "md" | "yml" | "yaml" => {
            Some(("prettier".into(), args(&["--stdin-filepath", rel])))
        }
        _ => None,
    }
}

/// `edition = "..."` of the nearest Cargo.toml above the file (this repo mixes 2021 and 2024).
fn edition(root: &Path, rel: &str) -> String {
    let mut dir = root.join(rel);
    dir.pop();
    loop {
        if let Ok(toml) = fs::read_to_string(dir.join("Cargo.toml")) {
            let found = toml.lines().find_map(|l| {
                let v = l.trim().strip_prefix("edition")?.trim_start().strip_prefix('=')?;
                Some(v.trim().trim_matches('"').to_string())
            });
            if let Some(e) = found {
                return e;
            }
        }
        if dir == root || !dir.pop() {
            return "2021".into();
        }
    }
}

/// Trailing spaces off every line, CRLF -> LF, exactly one final newline.
pub fn cleanup(text: &str) -> String {
    let body = text
        .replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n", body.trim_end_matches('\n'))
}

pub async fn format(root: PathBuf, rel: String, text: String) -> Result<Formatted, String> {
    let fallback = |text: &str| Formatted {
        text: cleanup(text),
        tool: "built-in cleanup".into(),
    };
    let Some((program, args)) = tool_for(&root, &rel) else {
        return Ok(fallback(&text));
    };
    match run(&program, &args, &root, &text).await {
        Ok(out) => Ok(Formatted { text: out, tool: program }),
        Err(RunError::Missing) => Ok(fallback(&text)),
        Err(RunError::Failed(msg)) => Err(format!("{program}: {msg}")),
    }
}

async fn run(program: &str, args: &[String], root: &Path, text: &str) -> Result<String, RunError> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root) // so rustfmt.toml / .prettierrc are found
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RunError::Missing,
            _ => RunError::Failed(e.to_string()),
        })?;

    // These formatters read all of stdin before writing, so writing first cannot deadlock.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes()).await;
    }
    let out = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| RunError::Failed("timed out".into()))?
        .map_err(|e| RunError::Failed(e.to_string()))?;

    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("failed");
        Err(RunError::Failed(first.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_normalizes_whitespace() {
        assert_eq!(cleanup("a  \r\nb\t\n\n\n"), "a\nb\n");
    }
}