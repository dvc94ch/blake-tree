use crate::{Hash, Mime, Range, Result};
use base64::engine::general_purpose::{GeneralPurpose, NO_PAD};
use base64::{alphabet, Engine};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const BASE64_ENGINE: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, NO_PAD);
const BYTES_LENGTH: usize = 43;
const BASE64_LENGTH: usize = 58;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StreamId {
    version: u8,
    hash: Hash,
    length: u64,
    mime: u16,
}

impl StreamId {
    pub fn new(hash: Hash, length: u64, mime: u16) -> Self {
        Self {
            version: 0,
            hash,
            length,
            mime,
        }
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub fn length(self) -> u64 {
        self.length
    }

    pub fn range(self) -> Range {
        Range::new(0, self.length)
    }

    pub fn mime(self) -> Mime {
        Mime::from_u16(self.mime).unwrap_or_default()
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let mime = Mime::from_path(path).unwrap_or_default();
        let mut f = BufReader::new(File::open(path)?);
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut f, &mut hasher)?;
        let length = hasher.count();
        let hash = hasher.finalize();
        Ok(StreamId::new(hash, length, mime as _))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        anyhow::ensure!(bytes.len() == BYTES_LENGTH);
        anyhow::ensure!(bytes[0] == 0);
        let mut hash = [0; 32];
        hash.copy_from_slice(&bytes[1..33]);
        let mut length = [0; 8];
        length.copy_from_slice(&bytes[33..41]);
        let length = u64::from_le_bytes(length);
        let mut mime = [0; 2];
        mime.copy_from_slice(&bytes[41..]);
        let mime = u16::from_le_bytes(mime);
        Ok(Self::new(hash.into(), length, mime))
    }

    pub fn to_bytes(self) -> [u8; BYTES_LENGTH] {
        let mut bytes = [0; BYTES_LENGTH];
        bytes[0] = self.version;
        bytes[1..33].copy_from_slice(&self.hash.as_bytes()[..]);
        bytes[33..41].copy_from_slice(&self.length.to_le_bytes()[..]);
        bytes[41..].copy_from_slice(&self.mime.to_le_bytes()[..]);
        bytes
    }

    pub fn from_base64(bytes64: &[u8]) -> Result<Self> {
        anyhow::ensure!(bytes64.len() == BASE64_LENGTH);
        let mut bytes = [0; BYTES_LENGTH];
        BASE64_ENGINE.decode_slice_unchecked(bytes64, &mut bytes)?;
        Self::from_bytes(&bytes[..])
    }

    pub fn to_base64(self) -> [u8; BASE64_LENGTH] {
        let bytes = self.to_bytes();
        debug_assert_eq!(bytes.len(), BYTES_LENGTH);
        let mut bytes64 = [0; BASE64_LENGTH];
        BASE64_ENGINE
            .encode_slice(&bytes[..], &mut bytes64)
            .unwrap();
        bytes64
    }
}

impl std::fmt::Debug for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StreamId({self})")
    }
}

impl std::fmt::Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base64 = self.to_base64();
        write!(f, "{}", std::str::from_utf8(&base64).unwrap())
    }
}

impl std::str::FromStr for StreamId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_base64(s.as_bytes())
    }
}

impl Serialize for StreamId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StreamId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map_err(|err| serde::de::Error::custom(format!("{err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_fmt() {
        let id = StreamId::new(blake3::hash(b""), 42, Mime::ApplicationTar as _);
        let s = id.to_string();
        println!("{}", s);
        let id2: StreamId = s.parse().unwrap();
        assert_eq!(id2, id);
    }
}
