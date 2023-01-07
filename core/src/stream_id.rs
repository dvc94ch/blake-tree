use crate::{Hash, Mime, Range, Result};
use base64::{
    alphabet,
    engine::fast_portable::{self, FastPortable},
};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const BASE64_ENGINE: FastPortable = FastPortable::from(&alphabet::URL_SAFE, fast_portable::PAD);
const BYTES_LENGTH: usize = 43;
const BASE64_LENGTH: usize = 60;

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
        base64::decode_engine_slice(bytes64, &mut bytes, &BASE64_ENGINE)?;
        Self::from_bytes(&bytes[..])
    }

    pub fn to_base64(self) -> [u8; BASE64_LENGTH] {
        let bytes = self.to_bytes();
        let mut bytes64 = [0; BASE64_LENGTH];
        base64::encode_engine_slice(&bytes[..], &mut bytes64, &BASE64_ENGINE);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_fmt() {
        let id = StreamId::new(blake3::hash(b""), 42, Mime::ApplicationTar as _);
        let id2: StreamId = id.to_string().parse().unwrap();
        assert_eq!(id2, id);
    }
}
