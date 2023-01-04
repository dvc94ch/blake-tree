mod hasher;
mod mime;
mod range;
mod stream_id;
mod tree;

pub use crate::hasher::{tree_hash, TreeHasher};
pub use crate::mime::{Mime, MimeType};
pub use crate::range::Range;
pub use crate::stream_id::StreamId;
pub use crate::tree::Tree;
pub use anyhow::Result;
pub use blake3::Hash;

pub const CHUNK_SIZE: u64 = 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};

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

    #[test]
    fn test_example() -> Result<()> {
        let path = "/tmp/file";
        let path2 = "/tmp/file2";
        std::fs::write(path, &[0x42; 2049][..])?;
        let range = Range::new(1024, 1024);

        let mut chunks = BufReader::new(File::open(path)?);
        let mut hasher = TreeHasher::new();
        std::io::copy(&mut chunks, &mut hasher)?;
        let tree = hasher.finalize();
        let slice = tree.encode_range(&range, &mut chunks);

        let mut chunks = BufWriter::new(File::create(path2)?);
        let mut tree2 = Tree::new(*tree.hash(), tree.length().unwrap());
        tree2.decode_range(range, &slice, &mut chunks)?;
        chunks.flush()?;

        let mut chunks = BufReader::new(File::open(path2)?);
        chunks.seek(SeekFrom::Start(range.offset()))?;
        let mut chunk = [0; 1024];
        chunks.read_exact(&mut chunk)?;
        assert_eq!(chunk, [0x42; 1024]);
        Ok(())
    }
}
