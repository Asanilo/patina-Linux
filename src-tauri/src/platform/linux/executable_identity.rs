//! Measure the open Linux running image, even when its original pathname is replaced.
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;

const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) fn running_sha256() -> Result<String, String> {
    // Follow this kernel-provided link intentionally. Reopening current_exe()'s path
    // would measure a replacement package instead of the image running this process.
    let file = File::open("/proc/self/exe")
        .map_err(|error| format!("cannot open running executable: {error}"))?;
    measure(file, MAX_EXECUTABLE_BYTES)
}

fn measure(mut file: File, limit: u64) -> Result<String, String> {
    let before = file.metadata().map_err(|error| error.to_string())?;
    if !before.is_file() || before.len() == 0 || before.len() > limit {
        return Err("running executable is not a bounded nonempty regular file".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > before.len() {
            return Err("running executable changed during measurement".into());
        }
        digest.update(&buffer[..read]);
    }
    let after = file.metadata().map_err(|error| error.to_string())?;
    if total != before.len()
        || after.len() != before.len()
        || after.modified().map_err(|error| error.to_string())?
            != before.modified().map_err(|error| error.to_string())?
    {
        return Err("running executable changed during measurement".into());
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRoot(std::path::PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random).unwrap();
            let path = std::env::temp_dir()
                .join(format!("patina-image-{:x}", u128::from_ne_bytes(random)));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn open_image_identity_survives_path_replacement_and_unlink() {
        let root = TestRoot::new();
        let path = root.path().join("daemon");
        std::fs::write(&path, b"original image").unwrap();
        let original = File::open(&path).unwrap();
        let replacement = root.path().join("replacement");
        std::fs::write(&replacement, b"new image").unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert_eq!(
            measure(original, 100).unwrap(),
            format!("{:x}", Sha256::digest(b"original image"))
        );
        assert_eq!(
            measure(File::open(path).unwrap(), 100).unwrap(),
            format!("{:x}", Sha256::digest(b"new image"))
        );
    }

    #[test]
    fn measurement_rejects_empty_oversized_and_nonregular_inputs() {
        let root = TestRoot::new();
        let path = root.path().join("daemon");
        std::fs::write(&path, b"").unwrap();
        assert!(measure(File::open(&path).unwrap(), 100).is_err());
        std::fs::write(&path, b"payload").unwrap();
        assert!(measure(File::open(&path).unwrap(), 3).is_err());
        assert!(measure(File::open(root.path()).unwrap(), 100).is_err());
    }
}
