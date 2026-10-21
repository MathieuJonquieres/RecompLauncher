//! File hashing and verification for user-provided dumps and downloads.
//!
//! Files are read once; every requested algorithm is updated in the same pass.
//! Nothing is copied, moved or uploaded.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::catalog::model::{AcceptedHash, HashAlgo};

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HashError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no supported hash algorithm requested")]
    NoAlgorithm,
}

/// Result of checking a user file against a manifest's accepted hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashVerdict {
    /// At least one accepted hash matched.
    Match {
        algo: HashAlgo,
        label: Option<String>,
    },
    /// The manifest publishes no hash: existence/extension checks only.
    NoAcceptedHashes,
    /// Hashes were computed but none matched.
    Mismatch {
        computed: BTreeMap<HashAlgo, String>,
        accepted: Vec<String>,
    },
}

impl HashVerdict {
    pub fn is_acceptable(&self) -> bool {
        matches!(self, Self::Match { .. } | Self::NoAcceptedHashes)
    }
}

/// Computes the requested hashes in one streaming pass.
pub fn compute_file(
    path: &Path,
    algorithms: &[HashAlgo],
) -> Result<BTreeMap<HashAlgo, String>, HashError> {
    if algorithms.is_empty() {
        return Err(HashError::NoAlgorithm);
    }

    let mut sha1 = algorithms.contains(&HashAlgo::Sha1).then(Sha1::new);
    let mut sha256 = algorithms.contains(&HashAlgo::Sha256).then(Sha256::new);
    let mut md5 = algorithms.contains(&HashAlgo::Md5).then(Md5::new);
    let mut crc32 = algorithms
        .contains(&HashAlgo::Crc32)
        .then(crc32fast::Hasher::new);

    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        if let Some(hasher) = sha1.as_mut() {
            hasher.update(chunk);
        }
        if let Some(hasher) = sha256.as_mut() {
            hasher.update(chunk);
        }
        if let Some(hasher) = md5.as_mut() {
            hasher.update(chunk);
        }
        if let Some(hasher) = crc32.as_mut() {
            hasher.update(chunk);
        }
    }

    let mut output = BTreeMap::new();
    if let Some(hasher) = sha1 {
        output.insert(HashAlgo::Sha1, hex(&hasher.finalize()));
    }
    if let Some(hasher) = sha256 {
        output.insert(HashAlgo::Sha256, hex(&hasher.finalize()));
    }
    if let Some(hasher) = md5 {
        output.insert(HashAlgo::Md5, hex(&hasher.finalize()));
    }
    if let Some(hasher) = crc32 {
        output.insert(HashAlgo::Crc32, format!("{:08x}", hasher.finalize()));
    }
    Ok(output)
}

/// Checks a file against every accepted hash. A single match is enough.
pub fn verify_file(path: &Path, accepted: &[AcceptedHash]) -> Result<HashVerdict, HashError> {
    if !path.is_file() {
        return Err(HashError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not a file", path.display()),
        )));
    }
    if accepted.is_empty() {
        return Ok(HashVerdict::NoAcceptedHashes);
    }

    let mut algorithms: Vec<HashAlgo> = accepted.iter().map(|hash| hash.algo).collect();
    algorithms.sort_unstable_by_key(|algo| algo.as_str());
    algorithms.dedup();
    let computed = compute_file(path, &algorithms)?;

    for candidate in accepted {
        if computed
            .get(&candidate.algo)
            .is_some_and(|value| value.eq_ignore_ascii_case(candidate.value.as_str()))
        {
            return Ok(HashVerdict::Match {
                algo: candidate.algo,
                label: candidate.label.clone(),
            });
        }
    }

    Ok(HashVerdict::Mismatch {
        computed,
        accepted: accepted
            .iter()
            .map(|hash| format!("{}:{}", hash.algo, hash.value.as_str()))
            .collect(),
    })
}

/// Verifies the SHA-256 of a downloaded file when the release publishes one.
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool, HashError> {
    let mut computed = compute_file(path, &[HashAlgo::Sha256])?;
    let value = computed.remove(&HashAlgo::Sha256).unwrap_or_default();
    Ok(value.eq_ignore_ascii_case(expected_hex))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::HashHex;

    fn temp_file(content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("dump.bin");
        std::fs::write(&path, content).expect("write");
        (dir, path)
    }

    #[test]
    fn computes_known_hashes() {
        let (_dir, path) = temp_file(b"abc");
        let hashes = compute_file(
            &path,
            &[
                HashAlgo::Sha1,
                HashAlgo::Sha256,
                HashAlgo::Md5,
                HashAlgo::Crc32,
            ],
        )
        .expect("hash");
        assert_eq!(
            hashes.get(&HashAlgo::Sha1).map(String::as_str),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(
            hashes.get(&HashAlgo::Sha256).map(String::as_str),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            hashes.get(&HashAlgo::Md5).map(String::as_str),
            Some("900150983cd24fb0d6963f7d28e17f72")
        );
        assert_eq!(
            hashes.get(&HashAlgo::Crc32).map(String::as_str),
            Some("352441c2")
        );
    }

    #[test]
    fn accepts_matching_hash() {
        let (_dir, path) = temp_file(b"abc");
        let accepted = vec![AcceptedHash {
            algo: HashAlgo::Sha1,
            value: HashHex::new("a9993e364706816aba3e25717850c26c9cd0d89d", HashAlgo::Sha1)
                .expect("hash"),
            label: Some("test".to_string()),
        }];
        let verdict = verify_file(&path, &accepted).expect("verify");
        assert!(verdict.is_acceptable());
        assert!(matches!(verdict, HashVerdict::Match { .. }));
    }

    #[test]
    fn rejects_wrong_hash() {
        let (_dir, path) = temp_file(b"abc");
        let accepted = vec![AcceptedHash {
            algo: HashAlgo::Sha1,
            value: HashHex::new("0".repeat(40), HashAlgo::Sha1).expect("hash"),
            label: None,
        }];
        let verdict = verify_file(&path, &accepted).expect("verify");
        assert!(!verdict.is_acceptable());
        assert!(matches!(verdict, HashVerdict::Mismatch { .. }));
    }

    #[test]
    fn empty_hash_list_only_checks_existence() {
        let (_dir, path) = temp_file(b"anything");
        assert_eq!(
            verify_file(&path, &[]).expect("verify"),
            HashVerdict::NoAcceptedHashes
        );
        let missing = path.with_extension("missing");
        assert!(verify_file(&missing, &[]).is_err());
    }

    #[test]
    fn sha256_helper() {
        let (_dir, path) = temp_file(b"abc");
        assert!(verify_sha256(
            &path,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .expect("hash"));
        assert!(!verify_sha256(&path, &"0".repeat(64)).expect("hash"));
    }
}
