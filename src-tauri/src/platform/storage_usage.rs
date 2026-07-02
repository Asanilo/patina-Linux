use std::fs;
use std::path::{Path, PathBuf};

pub fn path_size(path: &Path) -> Result<u64, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("failed to inspect `{}`: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0_u64;
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?
    {
        let entry = entry
            .map_err(|error| format!("failed to read entry in `{}`: {error}", path.display()))?;
        total = total.saturating_add(path_size(&entry.path())?);
    }
    Ok(total)
}

pub fn available_space_for(path: &Path) -> Result<u64, String> {
    let existing = nearest_existing_directory(path).ok_or_else(|| {
        format!(
            "storage target `{}` has no existing parent directory",
            path.display()
        )
    })?;
    fs2::available_space(&existing).map_err(|error| {
        format!(
            "failed to inspect available space for `{}`: {error}",
            existing.display()
        )
    })
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if let Ok(metadata) = fs::symlink_metadata(candidate) {
            if !metadata.file_type().is_symlink() && metadata.is_dir() {
                return Some(candidate.to_path_buf());
            }
        }
        current = candidate.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "patina-storage-usage-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(path: &Path, size: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1_u8; size]).unwrap();
    }

    #[test]
    fn managed_size_counts_regular_files() {
        let root = temp_dir("regular");
        write_file(&root.join("a"), 10);
        write_file(&root.join("nested/b"), 20);

        assert_eq!(path_size(&root).unwrap(), 30);
        assert!(available_space_for(&root.join("future/Patina")).unwrap() > 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn managed_size_skips_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("symlink");
        let outside = temp_dir("outside");
        write_file(&outside.join("large"), 100);
        symlink(&outside, root.join("linked")).unwrap();
        write_file(&root.join("owned"), 7);

        assert_eq!(path_size(&root).unwrap(), 7);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
