mod hasher;
mod manifest;
mod mime;
mod range;
mod store;
mod stream_id;
mod tree;

pub use crate::hasher::{tree_hash, TreeHasher};
pub use crate::manifest::Manifest;
pub use crate::mime::{Mime, MimeType};
pub use crate::range::Range;
pub use crate::store::{RangeReader, Stream, StreamEvent, StreamStorage};
pub use crate::stream_id::StreamId;
pub use crate::tree::{Insertion, Tree};
pub use anyhow::Result;
pub use blake3::Hash;

pub const CHUNK_SIZE: u64 = 1024;

#[cfg(test)]
mod tests {
    use super::*;

    // Interesting input lengths to run tests on.
    pub const TEST_CASES: &[u64] = &[
        0,
        1,
        10,
        CHUNK_SIZE - 1,
        CHUNK_SIZE,
        CHUNK_SIZE + 1,
        2 * CHUNK_SIZE - 1,
        2 * CHUNK_SIZE,
        2 * CHUNK_SIZE + 1,
        3 * CHUNK_SIZE - 1,
        3 * CHUNK_SIZE,
        3 * CHUNK_SIZE + 1,
        4 * CHUNK_SIZE - 1,
        4 * CHUNK_SIZE,
        4 * CHUNK_SIZE + 1,
        8 * CHUNK_SIZE - 1,
        8 * CHUNK_SIZE,
        8 * CHUNK_SIZE + 1,
        16 * CHUNK_SIZE - 1,
        16 * CHUNK_SIZE,
        16 * CHUNK_SIZE + 1,
    ];

    pub fn memory(i: u8) -> Result<sled::Db> {
        let path = std::env::temp_dir().join(i.to_string());
        Ok(sled::Config::new().path(path).temporary(true).open()?)
    }
}
