//! Build a flat `Item` list from one or more user-selected paths.

use std::path::{Path, PathBuf};

use super::protocol::Item;

/// Walk the given paths and produce a flat list of items.
///
/// For each input path:
/// - If it's a file, one item is produced with `rel_path = filename`.
/// - If it's a directory, the directory itself plus every descendant file and
///   subdirectory are produced, with `rel_path` rooted at the directory's
///   basename (e.g. dropping `/foo/bar` produces `bar/`, `bar/baz.txt`, …).
///
/// Paths that don't exist are silently skipped. Symlinks are followed only at
/// the top level; nested symlinks inside walked directories are not followed
/// to avoid loops.
pub fn walk_paths(paths: &[PathBuf]) -> Vec<(Item, Option<PathBuf>)> {
    let mut out: Vec<(Item, Option<PathBuf>)> = Vec::new();
    for path in paths {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        if meta.is_file() {
            let name = filename(path);
            out.push((
                Item {
                    rel_path: name,
                    size: meta.len(),
                    is_dir: false,
                },
                Some(path.clone()),
            ));
        } else if meta.is_dir() {
            let root_name = filename(path);
            // Include the root directory itself.
            out.push((
                Item {
                    rel_path: format!("{}/", root_name),
                    size: 0,
                    is_dir: true,
                },
                None,
            ));
            for entry in walkdir::WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let entry_path = entry.path();
                if entry_path == path {
                    continue;
                }
                let rel = match entry_path.strip_prefix(path) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let rel_str = to_rel_string(rel);
                if rel_str.is_empty() {
                    continue;
                }
                let ft = entry.file_type();
                if ft.is_dir() {
                    out.push((
                        Item {
                            rel_path: format!("{}/{}/", root_name, rel_str),
                            size: 0,
                            is_dir: true,
                        },
                        None,
                    ));
                } else if ft.is_file() {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push((
                        Item {
                            rel_path: format!("{}/{}", root_name, rel_str),
                            size,
                            is_dir: false,
                        },
                        Some(entry_path.to_path_buf()),
                    ));
                }
            }
        }
    }
    out
}

/// Basename of a path as a String, using the lossy conversion if needed.
fn filename(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".to_string())
}

/// Convert a relative path to a forward-slash string suitable for the wire.
fn to_rel_string(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Sanitize a rel_path coming off the wire so it cannot escape the save root.
/// Returns None if the path looks malicious.
pub fn sanitize_rel(rel: &str) -> Option<PathBuf> {
    let trimmed = rel.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        // Reject absolute / drive-prefixed segments.
        if part.contains(':') || part.contains('\\') {
            return None;
        }
        out.push(part);
    }
    Some(out)
}
