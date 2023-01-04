use crate::{Range, Tree, CHUNK_SIZE};
use std::io::Write;

#[derive(Clone)]
pub struct TreeHasher {
    stack: Vec<Tree>,
    chunk: [u8; 1024],
    chunk_length: usize,
    length: u64,
    chunks: usize,
}

impl Default for TreeHasher {
    fn default() -> Self {
        Self {
            stack: vec![],
            chunk: [0; 1024],
            chunk_length: 0,
            length: 0,
            chunks: 0,
        }
    }
}

impl TreeHasher {
    pub fn new() -> Self {
        Self::default()
    }

    fn fill_chunk(&mut self, bytes: &[u8]) {
        debug_assert!(self.chunk_length + bytes.len() <= CHUNK_SIZE as _);
        let chunk_length = self.chunk_length + bytes.len();
        self.chunk[self.chunk_length..chunk_length].copy_from_slice(bytes);
        self.chunk_length = chunk_length;
        self.length += bytes.len() as u64;
    }

    fn end_chunk(&mut self, finalize: bool) {
        let is_root = finalize && self.stack.is_empty();
        let range = Range::new(
            self.length - self.chunk_length as u64,
            self.chunk_length as _,
        );
        let hash = blake3::guts::ChunkState::new(range.index())
            .update(&self.chunk[..self.chunk_length])
            .finalize(is_root);
        let mut right = Tree::chunk(hash, range, is_root);
        self.chunks += 1;
        self.chunk_length = 0;

        let mut total_chunks = self.chunks;
        while total_chunks & 1 == 0 {
            let left = self.stack.pop().unwrap();
            let is_root = finalize && self.stack.is_empty();
            let hash = blake3::guts::parent_cv(left.hash(), right.hash(), is_root);
            let offset = left.range().offset();
            let length = left.range().length() + right.range().length();
            let range = Range::new(offset, length);
            right = Tree::tree(hash, range, is_root, left, right);
            total_chunks >>= 1;
        }
        self.stack.push(right);
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        let split = CHUNK_SIZE as usize - self.chunk_length;
        while split < bytes.len() as _ {
            let (chunk, rest) = bytes.split_at(split);
            self.fill_chunk(chunk);
            bytes = rest;
            self.end_chunk(false);
        }
        self.fill_chunk(bytes);
    }

    pub fn finalize(&mut self) -> Tree {
        self.end_chunk(true);
        let mut right = self.stack.pop().unwrap();
        while !self.stack.is_empty() {
            let left = self.stack.pop().unwrap();
            let is_root = self.stack.is_empty();
            let hash = blake3::guts::parent_cv(left.hash(), right.hash(), is_root);
            let offset = left.range().offset();
            let length = left.range().length() + right.range().length();
            let range = Range::new(offset, length);
            right = Tree::tree(hash, range, is_root, left, right);
        }
        right
    }
}

impl Write for TreeHasher {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn tree_hash(bytes: &[u8]) -> Tree {
    let mut hasher = TreeHasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_hasher() {
        let buf = [0x42; 65537];
        for &case in crate::tests::TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let hash = blake3::hash(bytes);
            let tree = tree_hash(bytes);
            assert_eq!(*tree.hash(), hash);
        }
    }
}
