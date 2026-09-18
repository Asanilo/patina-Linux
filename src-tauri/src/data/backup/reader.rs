//! Shared checked decoding for previews and full restore payloads.
use super::*;
use serde::de::DeserializeOwned;
use std::io::BufReader;

struct CheckedReader<R> {
    inner: R,
    hash: Hasher,
    bytes: u64,
}

impl<R: Read> Read for CheckedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.bytes += count as u64;
        if self.bytes > MAX_BACKUP_ENTRY_BYTES {
            return Err(std::io::Error::other("backup entry exceeds size limit"));
        }
        self.hash.update(&buffer[..count]);
        Ok(count)
    }
}

pub(super) fn checked<R: Read + Seek, T: DeserializeOwned>(
    archive: &mut ZipArchive<R>,
    checksums: &BackupArchiveChecksums,
    name: &str,
    path: &Path,
) -> Result<T, String> {
    verify_backup_checksums(checksums, &[], path)?;
    let expected = checksums
        .files
        .get(name)
        .ok_or_else(|| format!("missing checksum for {name}"))?;
    let entry = archive
        .by_name(name)
        .map_err(|error| format!("{name}: {error}"))?;
    if entry.size() > MAX_BACKUP_ENTRY_BYTES {
        return Err(format!("{name}: size limit exceeded"));
    }
    let mut reader = BufReader::with_capacity(
        64 * 1024,
        CheckedReader {
            inner: entry.take(MAX_BACKUP_ENTRY_BYTES + 1),
            hash: Hasher::new(),
            bytes: 0,
        },
    );
    let value = serde_json::from_reader(&mut reader)
        .map_err(|error| format!("invalid backup entry {name}: {error}"))?;
    let actual = format!("{:08x}", reader.into_inner().hash.finalize());
    if &actual != expected {
        return Err(format!("checksum mismatch for {name}"));
    }
    Ok(value)
}
