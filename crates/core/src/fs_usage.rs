//! Bounded disk-usage helpers used by the library and settings pages.

use std::path::Path;

/// Sums regular-file sizes below `root`.
///
/// Symbolic links are never followed, unreadable entries are skipped, and the
/// walk stops after [`MAX_ENTRIES`] entries so a huge or pathological tree
/// cannot stall the launcher.
pub fn dir_size(root: &Path) -> u64 {
    const MAX_ENTRIES: usize = 200_000;

    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;

    while let Some(path) = stack.pop() {
        if seen >= MAX_ENTRIES {
            break;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        seen += 1;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            stack.push(entry.path());
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_nested_regular_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("a/b")).expect("dirs");
        std::fs::write(dir.path().join("a/one.bin"), vec![1u8; 10]).expect("file");
        std::fs::write(dir.path().join("a/b/two.bin"), vec![2u8; 25]).expect("file");
        assert_eq!(dir_size(dir.path()), 35);
    }

    #[test]
    fn missing_root_is_zero() {
        assert_eq!(dir_size(Path::new("/definitely/not/here")), 0);
    }
}
