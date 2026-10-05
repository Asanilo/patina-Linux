//! A durable mask identity lets the installer preserve externally replaced masks.
//! Callers hold the profile installation lock and persist the plan before publish.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnedMask {
    temporary_name: String,
    device: u64,
    inode: u64,
}

impl OwnedMask {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self
            .temporary_name
            .strip_prefix(".patina-mask-")
            .and_then(|value| value.strip_suffix(".tmp"))
            .is_some_and(|value| {
                value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            || self.inode == 0
        {
            return Err("invalid standalone mask identity".into());
        }
        Ok(())
    }

    fn temporary(&self, config: &Path) -> Result<PathBuf, String> {
        self.validate()?;
        Ok(config.join("systemd/user").join(&self.temporary_name))
    }

    fn swap(&self, config: &Path) -> Result<PathBuf, String> {
        self.validate()?;
        Ok(config
            .join("systemd/user")
            .join(format!("{}.swap", self.temporary_name)))
    }

    fn verify_anchor(&self, config: &Path) -> Result<(), String> {
        let anchor = self.temporary(config)?;
        if !self.matches(&anchor)? {
            return Err("saved mask anchor changed; preserved".into());
        }
        let mut links = 1;
        for path in [
            config.join("systemd/user/patinad.service"),
            self.swap(config)?,
        ] {
            if self.matches(&path)? {
                links += 1;
            }
        }
        if fs::symlink_metadata(anchor)
            .map_err(|error| error.to_string())?
            .nlink()
            != links
        {
            return Err("saved mask has unrecognized links; preserved".into());
        }
        Ok(())
    }

    fn matches(&self, path: &Path) -> Result<bool, String> {
        self.validate()?;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        // SAFETY: geteuid has no arguments or side effects.
        Ok(metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
            && fs::read_link(path).map_err(|error| error.to_string())? == Path::new("/dev/null"))
    }

    pub(crate) fn is_installed(&self, config: &Path) -> Result<bool, String> {
        self.verify_anchor(config)?;
        self.matches(&config.join("systemd/user/patinad.service"))
    }
}

fn sync_parent(config: &Path) -> Result<(), String> {
    File::open(config.join("systemd/user"))
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

pub(crate) fn prepare_owned_mask(config: &Path, expected: &str) -> Result<OwnedMask, String> {
    validate_directories(config)?;
    if read_existing(&config.join("systemd/user/patinad.service"))?.as_deref() != Some(expected) {
        return Err("standalone unit changed before deactivation".into());
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let name = format!(".patina-mask-{:032x}.tmp", u128::from_ne_bytes(random));
    let path = config.join("systemd/user").join(&name);
    symlink("/dev/null", &path).map_err(|error| error.to_string())?;
    sync_parent(config)?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    Ok(OwnedMask {
        temporary_name: name,
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

pub(crate) fn publish_owned_mask(
    config: &Path,
    expected: &str,
    mask: &OwnedMask,
) -> Result<(), String> {
    validate_directories(config)?;
    if mask.is_installed(config)? {
        return Ok(());
    }
    let path = config.join("systemd/user/patinad.service");
    if read_existing(&path)?.as_deref() != Some(expected) {
        return Err(
            "standalone unit or prepared mask changed; existing configuration preserved".into(),
        );
    }
    let swap = mask.swap(config)?;
    if !mask.matches(&swap)? {
        // A separate hard link keeps the saved inode alive if someone removes
        // and recreates the active mask. Linux link() links the symlink itself.
        fs::hard_link(mask.temporary(config)?, &swap).map_err(|error| error.to_string())?;
    }
    mask.verify_anchor(config)?;
    if read_existing(&path)?.as_deref() != Some(expected) {
        return Err("standalone unit changed before mask publication".into());
    }
    fs::rename(swap, path).map_err(|error| error.to_string())?;
    sync_parent(config)
}

pub(crate) fn restore_owned_mask(
    config: &Path,
    expected: &str,
    mask: &OwnedMask,
) -> Result<(), String> {
    validate_directories(config)?;
    let path = config.join("systemd/user/patinad.service");
    if !mask.is_installed(config)? {
        if read_existing(&path)?.as_deref() == Some(expected) {
            return Ok(()); // An interrupted restore already published the unit.
        }
        return Err("the saved standalone mask no longer owns this unit".into());
    }
    let publication = mask.swap(config)?;
    match fs::symlink_metadata(&publication) {
        Ok(_) if mask.matches(&publication)? => {
            fs::remove_file(&publication).map_err(|error| error.to_string())?
        }
        Ok(_) => return Err("mask publication temporary changed; preserved".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    // Each write has a fresh name: an interrupted partial write cannot block a
    // retry or justify overwriting an unknown file left at an earlier name.
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let temporary = config.join("systemd/user").join(format!(
        ".patina-restore-{:032x}.tmp",
        u128::from_ne_bytes(random)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(expected.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    // The anchor stays alive throughout activation. Failed reloads can republish
    // the identical mask; a replacement mask cannot reuse that live inode.
    if !mask.is_installed(config)? || read_existing(&temporary)?.as_deref() != Some(expected) {
        return Err("standalone mask changed before restore".into());
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())?;
    sync_parent(config)
}

pub(crate) fn discard_owned_mask(config: &Path, mask: &OwnedMask) -> Result<(), String> {
    validate_directories(config)?;
    let path = mask.temporary(config)?;
    if fs::symlink_metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(());
    }
    if mask.is_installed(config)? || mask.matches(&mask.swap(config)?)? {
        return Err("saved mask is still published or pending; preserved".into());
    }
    fs::remove_file(path).map_err(|error| error.to_string())?;
    sync_parent(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    #[test]
    fn mask_identity_survives_restore_and_can_be_republished_after_failure() {
        let root = std::env::temp_dir().join(format!(
            "patina-mask-test-{}-{}",
            std::process::id(),
            crate::engine::runtime_context::now_ms()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        install_identical_or_new(&root, "known unit").unwrap();
        let mask = prepare_owned_mask(&root, "known unit").unwrap();
        publish_owned_mask(&root, "known unit", &mask).unwrap();
        publish_owned_mask(&root, "known unit", &mask).unwrap();
        assert!(mask.is_installed(&root).unwrap());
        restore_owned_mask(&root, "known unit", &mask).unwrap();
        restore_owned_mask(&root, "known unit", &mask).unwrap();
        assert!(!mask.is_installed(&root).unwrap());
        publish_owned_mask(&root, "known unit", &mask).unwrap();
        restore_owned_mask(&root, "known unit", &mask).unwrap();
        discard_owned_mask(&root, &mask).unwrap();
        discard_owned_mask(&root, &mask).unwrap();
        assert_eq!(
            read_existing(&root.join("systemd/user/patinad.service"))
                .unwrap()
                .as_deref(),
            Some("known unit")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_unit_and_another_mask_are_preserved() {
        let root = std::env::temp_dir().join(format!(
            "patina-mask-preserve-{}-{}",
            std::process::id(),
            crate::engine::runtime_context::now_ms()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        install_identical_or_new(&root, "known unit").unwrap();
        let mask = prepare_owned_mask(&root, "known unit").unwrap();
        let path = root.join("systemd/user/patinad.service");
        fs::write(&path, "custom unit").unwrap();
        assert!(publish_owned_mask(&root, "known unit", &mask).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "custom unit");
        fs::remove_file(&path).unwrap();
        symlink("/dev/null", &path).unwrap();
        assert!(!mask.is_installed(&root).unwrap());
        assert!(restore_owned_mask(&root, "known unit", &mask).is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("/dev/null"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn anchor_prevents_inode_reuse_when_the_published_mask_is_replaced() {
        let root = std::env::temp_dir().join(format!(
            "patina-mask-anchor-{}-{}",
            std::process::id(),
            crate::engine::runtime_context::now_ms()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        install_identical_or_new(&root, "known unit").unwrap();
        let mask = prepare_owned_mask(&root, "known unit").unwrap();
        publish_owned_mask(&root, "known unit", &mask).unwrap();
        let path = root.join("systemd/user/patinad.service");
        assert_eq!(fs::symlink_metadata(&path).unwrap().nlink(), 2);
        fs::remove_file(&path).unwrap();
        symlink("/dev/null", &path).unwrap();
        assert_ne!(fs::symlink_metadata(&path).unwrap().ino(), mask.inode);
        assert!(!mask.is_installed(&root).unwrap());
        assert!(restore_owned_mask(&root, "known unit", &mask).is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("/dev/null"));
        fs::hard_link(mask.temporary(&root).unwrap(), root.join("external-link")).unwrap();
        assert!(discard_owned_mask(&root, &mask).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
