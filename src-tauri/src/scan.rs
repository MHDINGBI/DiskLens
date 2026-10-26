use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// Fichiers >= ce seuil gardés individuellement; les autres sont agrégés par dossier.
pub const BIG_FILE: u64 = 4 * 1024 * 1024;
const CLUSTER: u64 = 4096;
// RECALL_ON_DATA_ACCESS | OFFLINE | RECALL_ON_OPEN: placeholders OneDrive, 0 octet sur disque.
const CLOUD_ATTRS: u32 = 0x0040_0000 | 0x0000_1000 | 0x0004_0000;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dir,
    File,
    Small,
}

pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub kind: Kind,
    pub size: u64,
    pub files: u32,
    pub newest: u32,
    /// mtime max des fichiers directement dans ce dossier (hors sous-dossiers).
    pub own_newest: u32,
    pub children: Vec<u32>,
}

pub struct Arena {
    pub nodes: Vec<Node>,
    pub scanned_at: u64,
    pub errors: u64,
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub dirs: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
}

struct FileRec {
    name: Box<str>,
    size: u64,
    mtime: u32,
}

struct Dir {
    name: Box<str>,
    small_size: u64,
    small_count: u32,
    own_newest: u32,
    big: Vec<FileRec>,
    dirs: Vec<Dir>,
}

fn mtime_secs(md: &fs::Metadata) -> u32 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs().min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

fn alloc_size(md: &fs::Metadata) -> u64 {
    if md.file_attributes() & CLOUD_ATTRS != 0 {
        return 0;
    }
    let len = md.len();
    len.div_ceil(CLUSTER) * CLUSTER
}

fn walk(path: &Path, name: Box<str>, depth: u32, prog: &Progress, cancel: &AtomicBool) -> Dir {
    let mut d = Dir {
        name,
        small_size: 0,
        small_count: 0,
        own_newest: 0,
        big: Vec::new(),
        dirs: Vec::new(),
    };
    if cancel.load(Ordering::Relaxed) {
        return d;
    }
    prog.dirs.fetch_add(1, Ordering::Relaxed);
    if depth <= 3 {
        if let Ok(mut c) = prog.current.try_lock() {
            *c = path.to_string_lossy().into_owned();
        }
    }
    let rd = match fs::read_dir(path) {
        Ok(r) => r,
        Err(_) => {
            prog.errors.fetch_add(1, Ordering::Relaxed);
            return d;
        }
    };
    let mut subs: Vec<(std::path::PathBuf, Box<str>)> = Vec::new();
    let (mut nfiles, mut nbytes) = (0u64, 0u64);
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        let ft = md.file_type();
        if ft.is_symlink() {
            continue; // jonctions / liens: évite les doubles comptages
        }
        let fname = e.file_name().to_string_lossy().into_owned().into_boxed_str();
        if ft.is_dir() {
            subs.push((e.path(), fname));
        } else {
            let size = alloc_size(&md);
            let mt = mtime_secs(&md);
            d.own_newest = d.own_newest.max(mt);
            nfiles += 1;
            nbytes += size;
            if size >= BIG_FILE {
                d.big.push(FileRec { name: fname, size, mtime: mt });
            } else {
                d.small_size += size;
                d.small_count += 1;
            }
        }
    }
    prog.files.fetch_add(nfiles, Ordering::Relaxed);
    prog.bytes.fetch_add(nbytes, Ordering::Relaxed);
    d.dirs = subs
        .into_par_iter()
        .map(|(p, n)| walk(&p, n, depth + 1, prog, cancel))
        .collect();
    d
}

fn flatten(d: Dir, parent: u32, nodes: &mut Vec<Node>) -> u32 {
    let idx = nodes.len() as u32;
    nodes.push(Node {
        name: d.name,
        parent,
        kind: Kind::Dir,
        size: 0,
        files: 0,
        newest: d.own_newest,
        own_newest: d.own_newest,
        children: Vec::new(),
    });
    let mut size = d.small_size;
    let mut files = d.small_count;
    let mut newest = d.own_newest;
    let mut children = Vec::with_capacity(d.big.len() + d.dirs.len() + 1);
    if d.small_count > 0 {
        let i = nodes.len() as u32;
        nodes.push(Node {
            name: format!("({} fichiers < 4 Mo)", d.small_count).into_boxed_str(),
            parent: idx,
            kind: Kind::Small,
            size: d.small_size,
            files: d.small_count,
            newest: d.own_newest,
            own_newest: d.own_newest,
            children: Vec::new(),
        });
        children.push(i);
    }
    for f in d.big {
        let i = nodes.len() as u32;
        size += f.size;
        files += 1;
        newest = newest.max(f.mtime);
        nodes.push(Node {
            name: f.name,
            parent: idx,
            kind: Kind::File,
            size: f.size,
            files: 1,
            newest: f.mtime,
            own_newest: f.mtime,
            children: Vec::new(),
        });
        children.push(i);
    }
    for sub in d.dirs {
        let i = flatten(sub, idx, nodes);
        let n = &nodes[i as usize];
        size += n.size;
        files += n.files;
        newest = newest.max(n.newest);
        children.push(i);
    }
    children.sort_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size));
    let n = &mut nodes[idx as usize];
    n.size = size;
    n.files = files;
    n.newest = newest;
    n.children = children;
    idx
}

pub fn scan_root(root: &str, prog: &Progress, cancel: &AtomicBool) -> Arena {
    let key = root.trim_end_matches('\\').to_string(); // "C:"
    let top = walk(Path::new(&format!("{key}\\")), key.clone().into_boxed_str(), 0, prog, cancel);
    let mut nodes = Vec::new();
    flatten(top, u32::MAX, &mut nodes);
    let scanned_at = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Arena { nodes, scanned_at, errors: prog.errors.load(Ordering::Relaxed) }
}

impl Arena {
    pub fn path_of(&self, mut idx: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        while idx != u32::MAX {
            let n = &self.nodes[idx as usize];
            parts.push(&n.name);
            idx = n.parent;
        }
        parts.reverse();
        if parts.len() == 1 {
            return format!("{}\\", parts[0]);
        }
        parts.join("\\")
    }

    /// Résout un chemin absolu (insensible à la casse) vers un index de nœud.
    pub fn resolve(&self, path: &str) -> Option<u32> {
        let mut comps = path.trim_end_matches('\\').split('\\').filter(|c| !c.is_empty());
        let first = comps.next()?;
        if !self.nodes.first()?.name.eq_ignore_ascii_case(first) {
            return None;
        }
        let mut cur = 0u32;
        for c in comps {
            let next = self.nodes[cur as usize]
                .children
                .iter()
                .copied()
                .find(|&i| self.nodes[i as usize].name.eq_ignore_ascii_case(c))?;
            cur = next;
        }
        Some(cur)
    }

    /// Détache un nœud et retire sa taille de toute la chaîne d'ancêtres.
    pub fn remove(&mut self, idx: u32) {
        if idx == 0 {
            return;
        }
        let (size, files, parent) = {
            let n = &self.nodes[idx as usize];
            (n.size, n.files, n.parent)
        };
        let mut p = parent;
        while p != u32::MAX {
            let n = &mut self.nodes[p as usize];
            n.size = n.size.saturating_sub(size);
            n.files = n.files.saturating_sub(files);
            p = n.parent;
        }
        self.nodes[parent as usize].children.retain(|&c| c != idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "scan réel du disque ; lancer explicitement si nécessaire"]
    fn scan_matches_reality() {
        let key = std::env::var("SCAN_TEST").unwrap_or(r"D:\INTELLIGENCE ARTICIELLE".to_string());
        let prog = Progress::default();
        let t = std::time::Instant::now();
        let top = walk(Path::new(&format!("{key}\\")), "X:".into(), 1, &prog, &AtomicBool::new(false));
        let mut nodes = Vec::new();
        flatten(top, u32::MAX, &mut nodes);
        println!("size={} files={} dirs={} errs={} in {:?}", nodes[0].size, nodes[0].files, prog.dirs.load(Ordering::Relaxed), prog.errors.load(Ordering::Relaxed), t.elapsed());
        assert!(nodes[0].size > 0);
    }
}
