//! A map of the notes: which notes exist, how they link, and where to draw them.

use std::collections::{HashMap, HashSet};

use super::store::Note;

#[derive(Debug, Clone)]
pub struct Node {
    /// Relative path, e.g. `notes/rust.md`.
    pub name: String,
    pub title: String,
    pub preview: String,
    /// Index into `Graph::groups`.
    pub group: usize,
    pub words: usize,
    pub degree: usize,
    /// Layout position, each in -1.0..=1.0.
    pub x: f32,
    pub y: f32,
    lower: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    /// true = a real `[[link]]`, false = faint link to the hub.
    pub explicit: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub groups: Vec<String>,
    /// The centre of the map (profile.md when it exists).
    pub hub: Option<usize>,
}

impl Graph {
    pub fn build(notes: &[Note]) -> Self {
        let mut groups: Vec<String> = notes.iter().map(|n| group_of(&n.name)).collect();
        groups.sort();
        groups.dedup();

        let mut nodes: Vec<Node> = notes
            .iter()
            .map(|note| {
                let group = group_of(&note.name);
                Node {
                    name: note.name.clone(),
                    title: title_of(note),
                    preview: preview_of(&note.content),
                    group: groups.iter().position(|g| *g == group).unwrap_or(0),
                    words: note.content.split_whitespace().count(),
                    degree: 0,
                    x: 0.0,
                    y: 0.0,
                    lower: format!("{}\n{}", note.name, note.content).to_lowercase(),
                }
            })
            .collect();

        // Names a [[link]] may use: full path, path without .md, or bare file name.
        let mut lookup: HashMap<String, usize> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            let full = n.name.to_lowercase();
            let stem = full.trim_end_matches(".md").to_string();
            let short = stem.rsplit('/').next().unwrap_or("").to_string();
            lookup.insert(full, i);
            lookup.insert(stem, i);
            lookup.entry(short).or_insert(i);
        }

        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut edges: Vec<Edge> = Vec::new();
        for (i, note) in notes.iter().enumerate() {
            for target in wiki_links(&note.content) {
                let key = target.trim_end_matches(".md").to_string();
                let Some(&j) = lookup.get(&target).or_else(|| lookup.get(&key)) else {
                    continue;
                };
                if i == j {
                    continue;
                }
                let pair = (i.min(j), i.max(j));
                if seen.insert(pair) {
                    edges.push(Edge { a: pair.0, b: pair.1, explicit: true });
                }
            }
        }

        // profile.md is always sent to the model, so it is the centre of the map.
        let hub = nodes
            .iter()
            .position(|n| n.name == "profile.md")
            .or(if nodes.is_empty() { None } else { Some(0) });

        if let Some(h) = hub {
            let mut linked = vec![false; nodes.len()];
            for e in &edges {
                linked[e.a] = true;
                linked[e.b] = true;
            }
            for i in 0..nodes.len() {
                if i != h && !linked[i] {
                    edges.push(Edge { a: i.min(h), b: i.max(h), explicit: false });
                }
            }
        }

        for e in &edges {
            nodes[e.a].degree += 1;
            nodes[e.b].degree += 1;
        }

        layout(&mut nodes, &edges, hub);
        Self { nodes, edges, groups, hub }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    pub fn neighbors(&self, i: usize) -> Vec<usize> {
        self.edges
            .iter()
            .filter_map(|e| {
                if e.a == i {
                    Some(e.b)
                } else if e.b == i {
                    Some(e.a)
                } else {
                    None
                }
            })
            .collect()
    }

    /// One flag per node: does it match the search text? Empty query matches all.
    pub fn matches(&self, query: &str) -> Vec<bool> {
        let q = query.trim().to_lowercase();
        self.nodes
            .iter()
            .map(|n| q.is_empty() || n.lower.contains(&q))
            .collect()
    }
}

fn group_of(name: &str) -> String {
    if let Some((folder, _)) = name.split_once('/') {
        return folder.to_string();
    }
    match name {
        "profile.md" | "user.md" => "core".into(),
        "inbox.md" => "inbox".into(),
        _ => "notes".into(),
    }
}

fn title_of(note: &Note) -> String {
    note.content
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let file = note.name.rsplit('/').next().unwrap_or(&note.name);
            file.trim_end_matches(".md").to_string()
        })
}

fn preview_of(content: &str) -> String {
    content
        .lines()
        .map(|l| l.trim().trim_start_matches(|c| matches!(c, '-' | '*' | '>' | ' ')))
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("")
        .to_string()
}

/// `[[note]]` and `[[note|label]]`, lowercased.
fn wiki_links(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let target = after[..end].split('|').next().unwrap_or("").trim().to_lowercase();
        if !target.is_empty() {
            out.push(target);
        }
        rest = &after[end + 2..];
    }
    out
}

/// Small deterministic force layout (Fruchterman-Reingold). The hub stays in the centre.
fn layout(nodes: &mut [Node], edges: &[Edge], hub: Option<usize>) {
    let n = nodes.len();
    if n == 0 {
        return;
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        let angle = i as f32 / n as f32 * std::f32::consts::TAU;
        let r = if Some(i) == hub { 0.0 } else { 0.6 };
        node.x = r * angle.cos();
        node.y = r * angle.sin();
    }

    let k = (4.0 / n as f32).sqrt() * 0.8;
    const STEPS: usize = 80;
    for step in 0..STEPS {
        let temp = 0.15 * (1.0 - step as f32 / STEPS as f32) + 0.01;
        let mut disp = vec![(0.0f32, 0.0f32); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let dx = nodes[i].x - nodes[j].x;
                let dy = nodes[i].y - nodes[j].y;
                let d = (dx * dx + dy * dy).sqrt().max(0.01);
                let f = k * k / d;
                let (fx, fy) = (dx / d * f, dy / d * f);
                disp[i].0 += fx;
                disp[i].1 += fy;
                disp[j].0 -= fx;
                disp[j].1 -= fy;
            }
        }
        for e in edges {
            let dx = nodes[e.a].x - nodes[e.b].x;
            let dy = nodes[e.a].y - nodes[e.b].y;
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = d * d / k * if e.explicit { 1.0 } else { 0.5 };
            let (fx, fy) = (dx / d * f, dy / d * f);
            disp[e.a].0 -= fx;
            disp[e.a].1 -= fy;
            disp[e.b].0 += fx;
            disp[e.b].1 += fy;
        }

        for (i, node) in nodes.iter_mut().enumerate() {
            if Some(i) == hub {
                node.x = 0.0;
                node.y = 0.0;
                continue;
            }
            let (dx, dy) = disp[i];
            let d = (dx * dx + dy * dy).sqrt().max(0.001);
            let len = d.min(temp);
            node.x += dx / d * len;
            node.y += dy / d * len;
            let r = (node.x * node.x + node.y * node.y).sqrt();
            if r > 1.0 {
                node.x /= r;
                node.y /= r;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str, content: &str) -> Note {
        Note { name: name.into(), content: content.into() }
    }

    #[test]
    fn wiki_links_become_edges_and_orphans_join_the_hub() {
        let g = Graph::build(&[
            note("inbox.md", "see [[user]]"),
            note("profile.md", "# Profile"),
            note("user.md", "hello"),
        ]);
        assert_eq!(g.hub, Some(1));
        assert!(g.edges.iter().any(|e| e.explicit));
        assert_eq!(g.nodes.len(), 3);
    }
}