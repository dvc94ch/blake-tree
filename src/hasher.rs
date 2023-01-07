use crate::{Hash, Insertion, Mime, Range, Result, StreamId, Tree, CHUNK_SIZE};
use std::io::Write;

#[derive(Clone)]
pub struct TreeHasher {
    batch: Vec<Insertion>,
    stack: Vec<Hash>,
    chunk: [u8; 1024],
    chunk_length: usize,
    length: u64,
    chunks: usize,
}

impl TreeHasher {
    pub fn new() -> Self {
        Self {
            batch: vec![],
            stack: vec![],
            chunk: [0; 1024],
            chunk_length: 0,
            length: 0,
            chunks: 0,
        }
    }

    fn fill_chunk(&mut self, bytes: &[u8]) {
        debug_assert!(self.chunk_length + bytes.len() <= CHUNK_SIZE as _);
        let chunk_length = self.chunk_length + bytes.len();
        self.chunk[self.chunk_length..chunk_length].copy_from_slice(bytes);
        self.chunk_length = chunk_length;
        self.length += bytes.len() as u64;
    }

    fn end_chunk(&mut self, finalize: bool) -> Result<()> {
        let is_root = finalize && self.stack.is_empty();
        let range = Range::new(
            self.length - self.chunk_length as u64,
            self.chunk_length as _,
        );
        let hash = blake3::guts::ChunkState::new(range.index())
            .update(&self.chunk[..self.chunk_length])
            .finalize(is_root);
        self.batch.push(Insertion::Chunk(hash));
        self.chunks += 1;
        self.chunk_length = 0;

        let mut right = hash;
        let mut total_chunks = self.chunks;
        while total_chunks & 1 == 0 {
            let left = self.stack.pop().unwrap();
            let is_root = finalize && self.stack.is_empty();
            let hash = blake3::guts::parent_cv(&left, &right, is_root);
            self.batch.push(Insertion::Parent(hash, left, right));
            right = hash;
            total_chunks >>= 1;
        }
        self.stack.push(right);
        Ok(())
    }

    pub fn update(&mut self, mut bytes: &[u8]) -> Result<()> {
        let split = CHUNK_SIZE as usize - self.chunk_length;
        while split < bytes.len() as _ {
            let (chunk, rest) = bytes.split_at(split);
            self.fill_chunk(chunk);
            bytes = rest;
            self.end_chunk(false)?;
        }
        self.fill_chunk(bytes);
        Ok(())
    }

    pub fn finalize(mut self, db: &sled::Db, mime: Mime) -> Result<Tree> {
        self.end_chunk(true)?;
        let mut right = self.stack.pop().unwrap();
        while !self.stack.is_empty() {
            let left = self.stack.pop().unwrap();
            let is_root = self.stack.is_empty();
            let hash = blake3::guts::parent_cv(&left, &right, is_root);
            self.batch.push(Insertion::Parent(hash, left, right));
            right = hash;
        }
        let id = StreamId::new(right, self.length, mime as _);
        let tree = Tree::open(db, id)?;
        tree.apply_batch(&self.batch)?;
        Ok(tree)
    }
}

impl Write for TreeHasher {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.update(buffer)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn tree_hash(db: &sled::Db, bytes: &[u8], mime: Mime) -> Result<Tree> {
    let mut hasher = TreeHasher::new();
    hasher.update(bytes)?;
    hasher.finalize(db, mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_hasher() -> Result<()> {
        let buf = [0x42; 65537];
        let db = crate::tests::memory(42)?;
        for &case in crate::tests::TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let hash = blake3::hash(bytes);
            let tree = tree_hash(&db, bytes, Mime::ApplicationOctetStream)?;
            assert_eq!(*tree.hash(), hash);
        }
        Ok(())
    }
}
