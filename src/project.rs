//! The open project folder: a scanned file tree, plus helpers that read files
//! and build prompt context from attached files and `@mentions`.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

/// Stop scanning after this many entries (keeps huge folders responsive).
const MAX_ENTRIES: usize = 3000;
/// Read at most this many bytes of any file.
const MAX_FILE_BYTES: u64 = 200_000;

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "venv",
    "env",
    "__pycache__",
    "vendor",
    "Pods",
    "DerivedData",
    "coverage",
];

/// Files with these extensions are hidden from the tree (not useful as text).
const BINARY_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "icns", "tiff", "heic", "mp3", "wav",
    "flac", "mp4", "mov", "avi", "mkv", "zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "dmg",
    "pkg", "exe", "dll", "so", "dylib", "a", "o", "class", "jar", "pdf", "ttf", "otf", "woff",
    "woff2", "sqlite", "db", "lock",
];

#[derive(Debug, Clone)]
pub struct Entry {
    /// Path relative to the project root, with `/` separators.
    pub rel: String,
    pub name: String,
    /// 0 for items directly in the root.
    pub depth: usize,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub entries: Vec<Entry>,
    /// True if the folder had more than `MAX_ENTRIES` items.
    pub truncated: bool,
    index: HashSet<String>,
}

/// File contents ready to be put in a prompt.
pub struct FileContext {
    pub text: String,
    pub included: Vec<String>,
    /// Files that were unreadable or did not fit in the size budget.
    pub skipped: Vec<String>,
}

impl Project {
    pub fn is_file(&self, rel: &str) -> bool {
        self.entries.iter().any(|e| !e.is_dir && e.rel == rel)
    }

    /// Every file with its main definitions, shrunk to fit `max_chars`.
    /// Files that don't fit with their definitions are listed as bare paths.
    pub fn repo_map(&self, max_chars: usize) -> String {
        let mut out = String::new();
        let mut hidden = 0usize;

        for entry in self.entries.iter().filter(|e| !e.is_dir) {
            let mut block = format!("{}\n", entry.rel);
            if is_code(&entry.rel) {
                if let Ok(text) = self.read(&entry.rel) {
                    for line in text.lines().filter(|l| is_signature(l)).take(16) {
                        let sig: String = line.trim().chars().take(90).collect();
                        block.push_str("  ");
                        block.push_str(&sig);
                        block.push('\n');
                    }
                }
            }
            if out.len() + block.len() <= max_chars {
                out.push_str(&block);
            } else {
                let bare = format!("{}\n", entry.rel);
                if out.len() + bare.len() <= max_chars {
                    out.push_str(&bare);
                } else {
                    hidden += 1;
                }
            }
        }
        if hidden > 0 {
            out.push_str(&format!("... and {hidden} more files not listed\n"));
        }
        out
    }

    /// README, manifests and entry points, used when the scout picks nothing.
    pub fn key_files(&self) -> Vec<String> {
        const NAMES: &[&str] = &[
            "README.md",
            "readme.md",
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "composer.json",
            "main.rs",
            "lib.rs",
            "main.py",
            "index.js",
            "index.ts",
        ];
        let mut hits: Vec<(usize, &Entry)> = self
            .entries
            .iter()
            .filter(|e| !e.is_dir && e.depth <= 1)
            .filter_map(|e| NAMES.iter().position(|n| *n == e.name).map(|p| (p, e)))
            .collect();
        hits.sort_by_key(|(p, e)| (*p, e.depth));
        hits.into_iter()
            .take(5)
            .map(|(_, e)| e.rel.clone())
            .collect()
    }

    pub fn scan(root: PathBuf) -> io::Result<Self> {
        if !root.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not a folder"));
        }
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root.to_string_lossy().to_string());

        let mut entries: Vec<Entry> = Vec::new();
        let mut truncated = false;

        // Folders first, then files, each alphabetical.
        let walker = WalkDir::new(&root)
            .min_depth(1)
            .max_depth(10)
            .sort_by(|a, b| {
                a.file_type()
                    .is_file()
                    .cmp(&b.file_type().is_file())
                    .then_with(|| a.file_name().cmp(b.file_name()))
            })
            .into_iter()
            .filter_entry(keep);

        for entry in walker.filter_map(Result::ok) {
            if entries.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            let is_dir = entry.file_type().is_dir();
            if !is_dir && is_binary(entry.path()) {
                continue;
            }
            let Ok(rel) = entry.path().strip_prefix(&root) else {
                continue;
            };
            entries.push(Entry {
                rel: rel.to_string_lossy().replace('\\', "/"),
                name: entry.file_name().to_string_lossy().to_string(),
                depth: entry.depth().saturating_sub(1),
                is_dir,
            });
        }

        let index = entries.iter().map(|e| e.rel.clone()).collect();
        Ok(Self {
            root,
            name,
            entries,
            truncated,
            index,
        })
    }

    pub fn contains(&self, rel: &str) -> bool {
        self.index.contains(rel)
    }

    /// Entries to show in the tree: everything whose parent folders are expanded.
    pub fn visible(&self, expanded: &HashSet<String>) -> Vec<&Entry> {
        let mut out = Vec::new();
        let mut hidden_below: Option<usize> = None;

        for entry in &self.entries {
            if let Some(depth) = hidden_below {
                if entry.depth > depth {
                    continue;
                }
                hidden_below = None;
            }
            out.push(entry);
            if entry.is_dir && !expanded.contains(&entry.rel) {
                hidden_below = Some(entry.depth);
            }
        }
        out
    }

    /// Read a file as text (first `MAX_FILE_BYTES` bytes).
    pub fn read(&self, rel: &str) -> Result<String, String> {
        if rel.split('/').any(|part| part == "..") {
            return Err("invalid path".into());
        }
        let path = self.root.join(rel);
        let file = File::open(&path).map_err(|e| format!("{rel}: {e}"))?;

        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{rel}: {e}"))?;

        if bytes.contains(&0) {
            return Err(format!("{rel} looks like a binary file"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// A plain list of paths so the model knows the project layout.
    pub fn tree_overview(&self, max_chars: usize) -> String {
        let mut out = String::new();
        let mut shown = 0;

        for entry in &self.entries {
            let line = if entry.is_dir {
                format!("{}/\n", entry.rel)
            } else {
                format!("{}\n", entry.rel)
            };
            if out.len() + line.len() > max_chars {
                break;
            }
            out.push_str(&line);
            shown += 1;
        }

        let hidden = self.entries.len() - shown;
        if hidden > 0 {
            out.push_str(&format!("... and {hidden} more entries\n"));
        }
        out
    }

    /// Files named in a message with `@path`, `@file.rs` (unique name) or
    /// `@folder` (every file inside it). Order is kept, duplicates removed.
    pub fn resolve_mentions(&self, message: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();

        for raw in message.split_whitespace() {
            let Some(token) = raw.strip_prefix('@') else {
                continue;
            };
            let token = token
                .trim_end_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | ')' | '?' | '!'))
                .trim_end_matches('/');
            if token.is_empty() {
                continue;
            }

            if let Some(entry) = self.entries.iter().find(|e| e.rel == token) {
                if entry.is_dir {
                    let prefix = format!("{}/", entry.rel);
                    out.extend(
                        self.entries
                            .iter()
                            .filter(|e| !e.is_dir && e.rel.starts_with(&prefix))
                            .map(|e| e.rel.clone()),
                    );
                } else {
                    out.push(entry.rel.clone());
                }
                continue;
            }

            let by_name: Vec<&Entry> = self
                .entries
                .iter()
                .filter(|e| !e.is_dir && e.name == token)
                .collect();
            if by_name.len() == 1 {
                out.push(by_name[0].rel.clone());
            }
        }

        let mut seen = HashSet::new();
        out.retain(|rel| seen.insert(rel.clone()));
        out
    }

    /// Autocomplete for `@query`: files and folders, best matches first.
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<&Entry> {
        let q = query.to_lowercase();

        let mut hits: Vec<(u8, &Entry)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let name = e.name.to_lowercase();
                let rel = e.rel.to_lowercase();
                let rank = if q.is_empty() {
                    2
                } else if name.starts_with(&q) {
                    0
                } else if name.contains(&q) {
                    1
                } else if rel.contains(&q) {
                    2
                } else {
                    return None;
                };
                Some((rank, e))
            })
            .collect();

        hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.rel.len().cmp(&b.1.rel.len())));
        hits.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// Read `files` and format them as fenced code blocks, within `budget_chars`.
    pub fn build_context(&self, files: &[String], budget_chars: usize) -> FileContext {
        let mut text = String::new();
        let mut used = 0usize;
        let mut included = Vec::new();
        let mut skipped = Vec::new();

        for rel in files {
            let Ok(content) = self.read(rel) else {
                skipped.push(rel.clone());
                continue;
            };

            let header = format!("### {rel}\n```{}\n", lang_for(rel));
            let footer = "\n```\n\n";
            let overhead = header.chars().count() + footer.chars().count() + 20;
            let remaining = budget_chars.saturating_sub(used);
            if remaining <= overhead + 100 {
                skipped.push(rel.clone());
                continue;
            }

            let room = remaining - overhead;
            let total = content.chars().count();
            let mut body: String = content.chars().take(room).collect();
            if total > room {
                body.push_str("\n[truncated]");
            }

            used += header.chars().count() + body.chars().count() + footer.chars().count();
            text.push_str(&header);
            text.push_str(&body);
            text.push_str(footer);
            included.push(rel.clone());
        }

        FileContext {
            text,
            included,
            skipped,
        }
    }
}

/// Skip hidden items and well-known build/dependency folders.
fn keep(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    if name.starts_with('.') {
        return false;
    }
    !(entry.file_type().is_dir() && IGNORED_DIRS.contains(&&*name))
}

fn is_binary(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| BINARY_EXT.contains(&ext.as_str()))
}

/// Language tag for the code fence.
pub fn lang_for(rel: &str) -> &str {
    let Some((_, ext)) = rel.rsplit_once('.') else {
        return "";
    };
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "md" => "markdown",
        "yml" | "yaml" => "yaml",
        "sh" | "zsh" | "bash" => "bash",
        "kt" => "kotlin",
        "rb" => "ruby",
        other => other,
    }
}
fn is_code(rel: &str) -> bool {
    const EXT: &[&str] = &[
        "rs", "py", "js", "jsx", "ts", "tsx", "php", "go", "java", "kt", "rb", "c", "cpp", "h",
        "cs", "swift", "vue", "svelte",
    ];
    rel.rsplit_once('.')
        .is_some_and(|(_, ext)| EXT.contains(&ext))
}

fn is_signature(line: &str) -> bool {
    const STARTS: &[&str] = &[
        "pub fn ",
        "pub async fn ",
        "fn ",
        "async fn ",
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "impl ",
        "pub mod ",
        "mod ",
        "pub type ",
        "def ",
        "async def ",
        "class ",
        "function ",
        "async function ",
        "export ",
        "func ",
        "interface ",
        "public ",
        "protected ",
        "private ",
    ];
    let t = line.trim_start();
    !t.starts_with("//") && STARTS.iter().any(|s| t.starts_with(s))
}
