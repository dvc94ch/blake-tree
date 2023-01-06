use crate::{Hash, Mime, Result};
use base64::{
    alphabet,
    engine::fast_portable::{self, FastPortable},
};

const BASE64_ENGINE: FastPortable = FastPortable::from(&alphabet::URL_SAFE, fast_portable::PAD);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StreamId {
    hash: Hash,
    length: u64,
    mime: u16,
}

impl StreamId {
    pub fn new(hash: Hash, length: u64, mime: u16) -> Self {
        Self { hash, length, mime }
    }

    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub fn length(self) -> u64 {
        self.length
    }

    pub fn mime(self) -> Mime {
        Mime::from_u16(self.mime).unwrap_or_default()
    }

    pub fn from_bytes(bytes: [u8; 42]) -> Self {
        let mut hash = [0; 32];
        hash.copy_from_slice(&bytes[..32]);
        let mut length = [0; 8];
        length.copy_from_slice(&bytes[32..40]);
        let length = u64::from_le_bytes(length);
        let mut mime = [0; 2];
        mime.copy_from_slice(&bytes[40..]);
        let mime = u16::from_le_bytes(mime);
        Self::new(hash.into(), length, mime)
    }

    pub fn to_bytes(self) -> [u8; 42] {
        let mut bytes = [0; 42];
        bytes[..32].copy_from_slice(&self.hash.as_bytes()[..]);
        bytes[32..40].copy_from_slice(&self.length.to_le_bytes()[..]);
        bytes[40..].copy_from_slice(&self.mime.to_le_bytes()[..]);
        bytes
    }

    pub fn from_base64(bytes64: [u8; 56]) -> Result<Self> {
        let mut bytes = [0; 42];
        base64::decode_engine_slice(bytes64, &mut bytes, &BASE64_ENGINE)?;
        Ok(Self::from_bytes(bytes))
    }

    pub fn to_base64(self) -> [u8; 56] {
        let bytes = self.to_bytes();
        let mut bytes64 = [0; 56];
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
        if s.len() != 56 {
            return Err(anyhow::anyhow!("invalid stream_id length {}", s.len()));
        }
        let mut bytes64 = [0; 56];
        bytes64.copy_from_slice(s.as_bytes());
        Self::from_base64(bytes64)
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
