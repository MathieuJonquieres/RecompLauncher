//! Secure archive extraction.
//!
//! Every entry name is validated as a [`SafeRelPath`] before touching the
//! filesystem. Symlinks, hard links, absolute paths, `..`, Windows device
//! names and duplicates are rejected. Extraction is bounded by total size,
//! entry count, per-file size and compression ratio.
//!
//! Supported formats: ZIP, tar, tar.gz/tgz, tar.xz/txz and 7z. Only extraction
//! is supported; the launcher never creates archives from game files.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use thiserror::Error;

const COPY_BUFFER: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_COMPONENT_BYTES: usize = 255;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArchiveError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),

    #[error("invalid or unsafe archive path: {0}")]
    UnsafePath(String),

    #[error("archive contains a symbolic or hard link: {0}")]
    LinkEntry(String),

    #[error("archive entry type is not allowed: {0}")]
    UnsupportedEntry(String),

    #[error("archive contains a duplicate entry: {0}")]
    DuplicateEntry(String),

    #[error("archive extraction limit exceeded: {0}")]
    LimitExceeded(String),

    #[error("destination is not empty: {0}")]
    DestinationNotEmpty(String),

    #[error("zip error: {0}")]
    Zip(String),

    #[error("7z error: {0}")]
    SevenZ(String),

    #[error("extraction cancelled")]
    Cancelled,
}

/// A normalized relative path that cannot escape its extraction root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SafeRelPath(String);

impl SafeRelPath {
    /// Validates and normalizes `raw` (accepts `/` and `\` separators).
    pub fn new(raw: &str) -> Result<Self, ArchiveError> {
        if raw.is_empty() {
            return Err(ArchiveError::UnsafePath("empty path".to_string()));
        }
        if raw.len() > MAX_PATH_BYTES {
            return Err(ArchiveError::UnsafePath("path is too long".to_string()));
        }
        if raw.contains('\0') {
            return Err(ArchiveError::UnsafePath("path contains NUL".to_string()));
        }

        let normalized = raw.replace('\\', "/");
        if normalized.starts_with('/') {
            return Err(ArchiveError::UnsafePath(format!("absolute path `{raw}`")));
        }
        if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
            return Err(ArchiveError::UnsafePath(format!("drive path `{raw}`")));
        }

        let mut parts = Vec::new();
        for component in normalized.split('/') {
            if component.is_empty() || component == "." {
                continue;
            }
            if component == ".." {
                return Err(ArchiveError::UnsafePath(format!(
                    "parent traversal `{raw}`"
                )));
            }
            if component.len() > MAX_COMPONENT_BYTES {
                return Err(ArchiveError::UnsafePath(format!(
                    "component is too long in `{raw}`"
                )));
            }
            if component.contains(':')
                || component.contains('<')
                || component.contains('>')
                || component.contains('"')
                || component.contains('|')
                || component.contains('?')
                || component.contains('*')
            {
                return Err(ArchiveError::UnsafePath(format!(
                    "invalid character in `{raw}`"
                )));
            }
            if component.ends_with(' ') || component.ends_with('.') {
                return Err(ArchiveError::UnsafePath(format!(
                    "trailing dot/space in `{raw}`"
                )));
            }
            let stem = component
                .split('.')
                .next()
                .unwrap_or(component)
                .to_ascii_lowercase();
            if is_windows_device_name(&stem) {
                return Err(ArchiveError::UnsafePath(format!(
                    "reserved device name in `{raw}`"
                )));
            }
            parts.push(component.to_string());
        }

        if parts.is_empty() {
            return Err(ArchiveError::UnsafePath(format!("empty path `{raw}`")));
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for component in self.0.split('/') {
            path.push(component);
        }
        path
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(self.0.as_str())
    }
}

impl std::fmt::Display for SafeRelPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn is_windows_device_name(stem: &str) -> bool {
    matches!(
        stem,
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

/// Hard limits applied to every extraction.
#[derive(Debug, Clone)]
pub struct ExtractionLimits {
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_entries: u64,
    /// Maximum `uncompressed / compressed` ratio per file. `0` disables the
    /// check (tar archives do not expose per-entry compressed sizes).
    pub max_compression_ratio: u64,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 8 * 1024 * 1024 * 1024,
            max_file_bytes: 4 * 1024 * 1024 * 1024,
            max_entries: 20_000,
            max_compression_ratio: 250,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarXz,
    SevenZ,
}

impl ArchiveFormat {
    pub fn detect(path: &Path) -> Result<Self, ArchiveError> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.ends_with(".zip") {
            return Ok(Self::Zip);
        }
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Ok(Self::TarGz);
        }
        if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            return Ok(Self::TarXz);
        }
        if name.ends_with(".tar") {
            return Ok(Self::Tar);
        }
        if name.ends_with(".7z") {
            return Ok(Self::SevenZ);
        }

        let mut magic = [0_u8; 8];
        let mut file = File::open(path)?;
        let read = file.read(&mut magic)?;
        let magic = &magic[..read];
        if magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06") {
            return Ok(Self::Zip);
        }
        if magic.starts_with(b"7z\xbc\xaf\x27\x1c") {
            return Ok(Self::SevenZ);
        }
        if magic.starts_with(&[0x1f, 0x8b]) {
            return Ok(Self::TarGz);
        }
        if magic.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
            return Ok(Self::TarXz);
        }
        Err(ArchiveError::UnsupportedFormat(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>")
                .to_string(),
        ))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractionReport {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

/// Extracts `archive_path` into `destination`.
///
/// The destination must be empty. On any error the caller is expected to
/// remove the destination (the installer extracts into a temporary directory
/// and only renames it after full success).
pub fn extract(
    archive_path: &Path,
    destination: &Path,
    limits: &ExtractionLimits,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<ExtractionReport, ArchiveError> {
    let format = ArchiveFormat::detect(archive_path)?;
    std::fs::create_dir_all(destination)?;
    if destination.read_dir()?.next().transpose()?.is_some() {
        return Err(ArchiveError::DestinationNotEmpty(
            destination.display().to_string(),
        ));
    }
    let destination = destination.canonicalize()?;

    let mut state = ExtractState {
        destination,
        limits: limits.clone(),
        cancel,
        seen: HashSet::new(),
        entries: 0,
        report: ExtractionReport::default(),
    };

    match format {
        ArchiveFormat::Zip => extract_zip(archive_path, &mut state)?,
        ArchiveFormat::Tar => {
            let file = File::open(archive_path)?;
            extract_tar(file, &mut state)?;
        }
        ArchiveFormat::TarGz => {
            let file = File::open(archive_path)?;
            extract_tar(flate2::read::GzDecoder::new(file), &mut state)?;
        }
        ArchiveFormat::TarXz => {
            let file = File::open(archive_path)?;
            extract_tar(xz2::read::XzDecoder::new(file), &mut state)?;
        }
        ArchiveFormat::SevenZ => extract_7z(archive_path, &mut state)?,
    }

    Ok(state.report)
}

struct ExtractState {
    destination: PathBuf,
    limits: ExtractionLimits,
    cancel: Option<Arc<AtomicBool>>,
    seen: HashSet<PathBuf>,
    entries: u64,
    report: ExtractionReport,
}

impl ExtractState {
    fn check_cancel(&self) -> Result<(), ArchiveError> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            return Err(ArchiveError::Cancelled);
        }
        Ok(())
    }

    fn begin_entry(&mut self, raw: &str) -> Result<SafeRelPath, ArchiveError> {
        self.check_cancel()?;
        self.entries = self.entries.saturating_add(1);
        if self.entries > self.limits.max_entries {
            return Err(ArchiveError::LimitExceeded(format!(
                "more than {} entries",
                self.limits.max_entries
            )));
        }
        let path = SafeRelPath::new(raw)?;
        let normalized = path.to_path_buf();
        if !self.seen.insert(normalized) {
            return Err(ArchiveError::DuplicateEntry(path.to_string()));
        }
        Ok(path)
    }

    fn create_dir(&mut self, path: &SafeRelPath) -> Result<(), ArchiveError> {
        self.check_cancel()?;
        let target = self.destination.join(path.to_path_buf());
        std::fs::create_dir_all(&target)?;
        self.report.dirs = self.report.dirs.saturating_add(1);
        Ok(())
    }

    fn write_file(
        &mut self,
        path: &SafeRelPath,
        declared_size: u64,
        compressed_size: Option<u64>,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveError> {
        self.check_cancel()?;
        if declared_size > self.limits.max_file_bytes {
            return Err(ArchiveError::LimitExceeded(format!(
                "file `{path}` declares {declared_size} bytes (max {})",
                self.limits.max_file_bytes
            )));
        }
        if let Some(compressed) = compressed_size {
            if compressed > 0 && self.limits.max_compression_ratio > 0 {
                let maximum = compressed.saturating_mul(self.limits.max_compression_ratio);
                if declared_size > maximum {
                    return Err(ArchiveError::LimitExceeded(format!(
                        "compression ratio too high for `{path}`"
                    )));
                }
            }
        }

        let target = self.destination.join(path.to_path_buf());
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;

        let mut buffer = vec![0_u8; COPY_BUFFER];
        let mut written = 0_u64;
        loop {
            self.check_cancel()?;
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            written = written.saturating_add(read as u64);
            if written > self.limits.max_file_bytes {
                return Err(ArchiveError::LimitExceeded(format!(
                    "file `{path}` exceeds {} bytes",
                    self.limits.max_file_bytes
                )));
            }
            let new_total = self.report.bytes.saturating_add(written);
            if new_total > self.limits.max_total_bytes {
                return Err(ArchiveError::LimitExceeded(format!(
                    "total extracted size exceeds {} bytes",
                    self.limits.max_total_bytes
                )));
            }
            file.write_all(&buffer[..read])?;
        }
        file.flush()?;
        self.report.bytes = self.report.bytes.saturating_add(written);
        self.report.files = self.report.files.saturating_add(1);
        Ok(())
    }
}

fn extract_zip(path: &Path, state: &mut ExtractState) -> Result<(), ArchiveError> {
    let file = File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ArchiveError::Zip(error.to_string()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ArchiveError::Zip(error.to_string()))?;
        let raw = entry.name().to_string();
        let is_dir = entry.is_dir();
        if !is_dir {
            if let Some(mode) = entry.unix_mode() {
                if mode & 0o170000 == 0o120000 {
                    return Err(ArchiveError::LinkEntry(raw));
                }
            }
        }
        let path = state.begin_entry(&raw)?;
        if is_dir {
            state.create_dir(&path)?;
        } else {
            let declared = entry.size();
            let compressed = entry.compressed_size();
            state.write_file(&path, declared, Some(compressed), &mut entry)?;
        }
    }
    Ok(())
}

fn extract_tar<R: Read>(reader: R, state: &mut ExtractState) -> Result<(), ArchiveError> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        let raw = entry
            .path()?
            .to_str()
            .ok_or_else(|| ArchiveError::UnsafePath("non-UTF-8 tar path".to_string()))?
            .to_string();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(ArchiveError::LinkEntry(raw));
        }
        let path = state.begin_entry(&raw)?;
        if entry_type.is_dir() {
            state.create_dir(&path)?;
        } else if entry_type.is_file() {
            let declared = entry.size();
            state.write_file(&path, declared, None, &mut entry)?;
        } else {
            return Err(ArchiveError::UnsupportedEntry(format!(
                "{raw} ({entry_type:?})"
            )));
        }
    }
    Ok(())
}

fn extract_7z(path: &Path, state: &mut ExtractState) -> Result<(), ArchiveError> {
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|error| ArchiveError::SevenZ(error.to_string()))?;
    let mut failure = None;
    reader
        .for_each_entries(|entry, data| {
            if failure.is_some() {
                return Ok(false);
            }
            let result = (|| -> Result<(), ArchiveError> {
                if entry.is_anti_item() {
                    return Err(ArchiveError::UnsupportedEntry(entry.name().to_string()));
                }
                let raw = entry.name().to_string();
                let path = state.begin_entry(&raw)?;
                if entry.is_directory() {
                    state.create_dir(&path)
                } else {
                    state.write_file(&path, entry.size(), Some(entry.compressed_size), data)
                }
            })();
            match result {
                Ok(()) => Ok(true),
                Err(error) => {
                    failure = Some(error);
                    Ok(false)
                }
            }
        })
        .map_err(|error| ArchiveError::SevenZ(error.to_string()))?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Component;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn zip_file(path: &Path, entries: &[(&str, &[u8])]) {
        use zip::write::SimpleFileOptions;
        let file = File::create(path).expect("zip file");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in entries {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(content).expect("write zip");
        }
        writer.finish().expect("finish zip");
    }

    #[test]
    fn safe_path_accepts_normal_paths() {
        assert_eq!(
            SafeRelPath::new("folder/game.exe").expect("path").as_str(),
            "folder/game.exe"
        );
        assert_eq!(
            SafeRelPath::new("./folder\\game.exe")
                .expect("path")
                .as_str(),
            "folder/game.exe"
        );
        assert_eq!(
            SafeRelPath::new("folder/").expect("path").as_str(),
            "folder"
        );
    }

    #[test]
    fn safe_path_rejects_escapes() {
        for candidate in [
            "../evil",
            "folder/../../evil",
            "/absolute",
            "C:/windows",
            "folder\\..\\evil",
            "CON",
            "folder/NUL.txt",
            "folder/file:stream",
            "folder/trailing. ",
        ] {
            assert!(
                SafeRelPath::new(candidate).is_err(),
                "{candidate} should be rejected"
            );
        }
    }

    #[test]
    fn extracts_safe_zip() {
        let dir = temp_dir();
        let archive = dir.path().join("game.zip");
        zip_file(
            &archive,
            &[("folder/game.txt", b"hello"), ("folder/data.bin", b"1234")],
        );
        let destination = dir.path().join("out");
        let report =
            extract(&archive, &destination, &ExtractionLimits::default(), None).expect("extract");
        assert_eq!(report.files, 2);
        assert_eq!(report.bytes, 9);
        assert_eq!(
            std::fs::read_to_string(destination.join("folder/game.txt")).expect("read"),
            "hello"
        );
    }

    #[test]
    fn rejects_zip_traversal() {
        let dir = temp_dir();
        let archive = dir.path().join("evil.zip");
        zip_file(&archive, &[("../evil.txt", b"owned")]);
        let destination = dir.path().join("out");
        let error = extract(&archive, &destination, &ExtractionLimits::default(), None)
            .expect_err("must reject traversal");
        assert!(matches!(error, ArchiveError::UnsafePath(_)));
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn rejects_zip_bomb_ratio() {
        let dir = temp_dir();
        let archive = dir.path().join("bomb.zip");
        let payload = vec![b'a'; 1024 * 1024];
        zip_file(&archive, &[("bomb.bin", &payload)]);
        let limits = ExtractionLimits {
            max_compression_ratio: 2,
            ..ExtractionLimits::default()
        };
        let error = extract(&archive, &dir.path().join("out"), &limits, None)
            .expect_err("must reject ratio");
        assert!(matches!(error, ArchiveError::LimitExceeded(_)));
    }

    #[test]
    fn rejects_tar_traversal_and_links() {
        let dir = temp_dir();
        let archive = dir.path().join("evil.tar");
        let file = File::create(&archive).expect("tar file");
        let mut builder = tar::Builder::new(file);

        let mut header = tar::Header::new_gnu();
        // `Header::set_path` refuses `..`, so build a valid header and then
        // overwrite the raw name field exactly like a malicious archive would.
        header.set_path("safe.txt").expect("set path");
        header.as_mut_bytes()[..11].copy_from_slice(b"../evil.txt");
        header.set_size(5);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &b"owned"[..]).expect("append");

        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_path("link").expect("link path");
        link.set_link_name("outside").expect("link target");
        link.set_size(0);
        link.set_mode(0o777);
        link.set_cksum();
        builder.append(&link, io::empty()).expect("append link");
        builder.finish().expect("finish tar");

        let error = extract(
            &archive,
            &dir.path().join("out"),
            &ExtractionLimits::default(),
            None,
        )
        .expect_err("must reject traversal");
        assert!(matches!(error, ArchiveError::UnsafePath(_)));
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn rejects_tar_symlink_even_with_safe_name() {
        let dir = temp_dir();
        let archive = dir.path().join("link.tar");
        let file = File::create(&archive).expect("tar file");
        let mut builder = tar::Builder::new(file);
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_path("link").expect("link path");
        link.set_link_name("outside").expect("link target");
        link.set_size(0);
        link.set_mode(0o777);
        link.set_cksum();
        builder.append(&link, io::empty()).expect("append link");
        builder.finish().expect("finish tar");

        let error = extract(
            &archive,
            &dir.path().join("out"),
            &ExtractionLimits::default(),
            None,
        )
        .expect_err("must reject symlink");
        assert!(matches!(error, ArchiveError::LinkEntry(_)));
    }

    #[test]
    fn extracts_safe_7z() {
        let dir = temp_dir();
        let archive = dir.path().join("game.7z");
        let mut writer = sevenz_rust::SevenZWriter::create(&archive).expect("7z writer");
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = "game.txt".to_string();
        entry.has_stream = true;
        writer
            .push_archive_entry(entry, Some(Cursor::new(b"seven".to_vec())))
            .expect("push");
        writer.finish().expect("finish 7z");

        let destination = dir.path().join("out");
        let report = extract(&archive, &destination, &ExtractionLimits::default(), None)
            .expect("extract 7z");
        assert_eq!(report.files, 1);
        assert_eq!(
            std::fs::read_to_string(destination.join("game.txt")).expect("read"),
            "seven"
        );
    }

    #[test]
    fn rejects_7z_traversal() {
        let dir = temp_dir();
        let archive = dir.path().join("evil.7z");
        let mut writer = sevenz_rust::SevenZWriter::create(&archive).expect("7z writer");
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = "../evil.txt".to_string();
        entry.has_stream = true;
        writer
            .push_archive_entry(entry, Some(Cursor::new(b"owned".to_vec())))
            .expect("push");
        writer.finish().expect("finish 7z");

        let error = extract(
            &archive,
            &dir.path().join("out"),
            &ExtractionLimits::default(),
            None,
        )
        .expect_err("must reject traversal");
        assert!(matches!(error, ArchiveError::UnsafePath(_)));
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn rejects_non_empty_destination() {
        let dir = temp_dir();
        let archive = dir.path().join("game.zip");
        zip_file(&archive, &[("game.txt", b"hello")]);
        let destination = dir.path().join("out");
        std::fs::create_dir_all(&destination).expect("mkdir");
        std::fs::write(destination.join("existing"), b"x").expect("write");
        let error = extract(&archive, &destination, &ExtractionLimits::default(), None)
            .expect_err("must reject non-empty destination");
        assert!(matches!(error, ArchiveError::DestinationNotEmpty(_)));
    }

    #[test]
    fn cancellation_stops_extraction() {
        let dir = temp_dir();
        let archive = dir.path().join("game.zip");
        zip_file(&archive, &[("game.txt", b"hello")]);
        let cancel = Arc::new(AtomicBool::new(true));
        let error = extract(
            &archive,
            &dir.path().join("out"),
            &ExtractionLimits::default(),
            Some(cancel),
        )
        .expect_err("must cancel");
        assert!(matches!(error, ArchiveError::Cancelled));
    }

    proptest::proptest! {
        #[test]
        fn generated_paths_never_escape(raw in ".*") {
            if let Ok(path) = SafeRelPath::new(&raw) {
                let normalized = path.to_path_buf();
                let escapes = normalized.components().any(|component| {
                    matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
                });
                proptest::prop_assert!(!normalized.is_absolute());
                proptest::prop_assert!(!escapes);
            }
        }
    }
}
