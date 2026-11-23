//! Safe names for files taken out of archives or put into them.

use std::collections::HashSet;
use std::path::PathBuf;

/// Characters Windows does not allow in file names (and therefore avoided everywhere).
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*'];
/// Device names Windows reserves, with any extension.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];
const MAX_COMPONENT_CHARS: usize = 200;
const MAX_DEPTH: usize = 64;

/// A relative path inside the output folder for an archive entry name, or `None` when the
/// name must be skipped (it is empty or tries to leave the folder with `..`). Absolute
/// names are made relative; characters Windows rejects become `_`.
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    let unified = name.replace('\\', "/");
    let mut path = PathBuf::new();
    let mut depth = 0;
    for (index, part) in unified.split('/').enumerate() {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        // `C:` at the start of an absolute Windows name.
        if index == 0 && part.len() == 2 && part.ends_with(':') {
            continue;
        }
        path.push(clean_component(part));
        depth += 1;
        if depth > MAX_DEPTH {
            return None;
        }
    }
    (depth > 0).then_some(path)
}

/// One name, usable on every system.
pub fn clean_component(part: &str) -> String {
    let mut clean: String = part
        .chars()
        .map(|c| {
            if c.is_control() || FORBIDDEN.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .take(MAX_COMPONENT_CHARS)
        .collect();
    // Windows drops trailing dots and spaces, which could merge two names.
    while clean.ends_with(['.', ' ']) {
        clean.pop();
    }
    if clean.is_empty() {
        clean.push('_');
    }
    let stem = clean
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        clean.insert(0, '_');
    }
    clean
}

/// Gives each entry of a new archive a distinct name: `a.txt`, `a (2).txt`...
#[derive(Debug, Default)]
pub struct UniqueNames {
    used: HashSet<String>,
}

impl UniqueNames {
    pub fn take(&mut self, name: &str) -> String {
        let name = clean_component(name);
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
            _ => (name.clone(), String::new()),
        };
        let mut candidate = name;
        let mut n = 2;
        // Case-insensitive: Windows and macOS would see `A.txt` and `a.txt` as one file.
        while !self.used.insert(candidate.to_lowercase()) {
            candidate = format!("{stem} ({n}){ext}");
            n += 1;
        }
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(name: &str) -> Option<String> {
        safe_relative(name).map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    #[test]
    fn keeps_entries_inside_the_folder() {
        assert_eq!(rel("docs/a.txt").as_deref(), Some("docs/a.txt"));
        assert_eq!(rel("/etc/passwd").as_deref(), Some("etc/passwd"));
        assert_eq!(rel("C:\\Windows\\x.dll").as_deref(), Some("Windows/x.dll"));
        assert_eq!(rel("./a//b/").as_deref(), Some("a/b"));
        assert_eq!(rel("../evil"), None);
        assert_eq!(rel("a/../../evil"), None);
        assert_eq!(rel("a\\..\\b"), None);
        assert_eq!(rel(""), None);
        assert_eq!(rel("/"), None);
        assert_eq!(rel(&"a/".repeat(100)), None);
    }

    #[test]
    fn cleans_names_for_windows() {
        assert_eq!(clean_component("a:b?.txt"), "a_b_.txt");
        assert_eq!(clean_component("nome. "), "nome");
        assert_eq!(clean_component("CON.txt"), "_CON.txt");
        assert_eq!(clean_component("..."), "_");
        assert_eq!(clean_component("tab\there"), "tab_here");
    }

    #[test]
    fn numbers_repeated_names() {
        let mut names = UniqueNames::default();
        assert_eq!(names.take("a.txt"), "a.txt");
        assert_eq!(names.take("A.TXT"), "A (2).TXT");
        assert_eq!(names.take("a.txt"), "a (3).txt");
        assert_eq!(names.take(".bashrc"), ".bashrc");
        assert_eq!(names.take(".bashrc"), ".bashrc (2)");
    }
}
