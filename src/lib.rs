use anyhow::Result;
use blake3::Hash;
use std::io::{Read, Seek, SeekFrom, Write};

pub const CHUNK_SIZE: u64 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    offset: u64,
    length: u64,
}

impl Range {
    pub fn new(offset: u64, length: u64) -> Self {
        //debug_assert!(offset % CHUNK_SIZE == 0);
        Self { offset, length }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn end(&self) -> u64 {
        self.offset + self.length
    }

    pub fn index(&self) -> u64 {
        self.offset / CHUNK_SIZE
    }

    pub fn num_chunks(&self) -> u64 {
        if self.is_chunk() {
            1
        } else {
            (self.length + CHUNK_SIZE - 1) / CHUNK_SIZE
        }
    }

    pub fn is_chunk(&self) -> bool {
        self.length <= CHUNK_SIZE
    }

    pub fn split_at(&self, i: u64) -> Option<(Range, Range)> {
        assert!(i > 0);
        let at = i * CHUNK_SIZE;
        if self.length > at {
            let first = Range::new(self.offset, at);
            let second = Range::new(self.offset + at, self.length - at);
            Some((first, second))
        } else {
            None
        }
    }

    pub fn split(&self) -> Option<(Range, Range)> {
        if self.length > CHUNK_SIZE {
            let n = (self.length - 1) / CHUNK_SIZE;
            let n2 = n.ilog2();
            let i = 1 << n2; // 2^n2
            self.split_at(i)
        } else {
            None
        }
    }

    pub fn intersects(&self, other: &Range) -> bool {
        self == other ||
        // easier to write down when it does not intersect and invert
        // !(self.end() <= other.offset() || self.offset() >= other.end())
        // and use boolean algebra to simplify expression
        self.end() > other.offset() && self.offset() < other.end()
    }

    pub fn encoded_size(&self) -> u64 {
        const HEADER_SIZE: u64 = 8;
        const PARENT_SIZE: u64 = 32 * 2;
        let num_chunks = self.num_chunks();
        // num parents always one less than num chunks
        let num_parents = num_chunks - 1;
        HEADER_SIZE + PARENT_SIZE * num_parents + CHUNK_SIZE * num_chunks
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}..{}", self.offset, self.end())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Children {
    None,
    Data,
    Tree(Box<(Tree, Tree)>),
}

impl Children {
    fn is_none(&self) -> bool {
        self == &Self::None
    }

    fn as_deref(&self) -> Option<&(Tree, Tree)> {
        match self {
            Self::Tree(nodes) => Some(&**nodes),
            _ => None,
        }
    }

    fn as_deref_mut(&mut self) -> Option<&mut (Tree, Tree)> {
        match self {
            Self::Tree(nodes) => Some(&mut **nodes),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tree {
    range: Range,
    is_root: bool,
    hash: Hash,
    children: Children,
}

impl Tree {
    pub fn new(hash: Hash, length: u64) -> Self {
        Self {
            range: Range::new(0, length),
            is_root: true,
            hash,
            children: Children::None,
        }
    }

    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub fn range(&self) -> &Range {
        &self.range
    }

    pub fn is_root(&self) -> bool {
        self.is_root
    }

    pub fn is_chunk(&self) -> bool {
        self.range.is_chunk()
    }

    pub fn left(&self) -> Option<&Tree> {
        self.children.as_deref().map(|(left, _)| left)
    }

    pub fn right(&self) -> Option<&Tree> {
        self.children.as_deref().map(|(_, right)| right)
    }

    pub fn complete(&self) -> bool {
        if self.is_chunk() {
            !self.children.is_none()
        } else if let Some((left, right)) = self.children.as_deref() {
            left.complete() && right.complete()
        } else {
            false
        }
    }

    pub fn last_chunk(&self) -> &Tree {
        if let Some(right) = self.right() {
            right.last_chunk()
        } else {
            self
        }
    }

    pub fn length(&self) -> Option<u64> {
        let last = self.last_chunk();
        if last.children == Children::Data {
            Some(last.range().end())
        } else {
            None
        }
    }

    fn inner_ranges(&self, ranges: &mut Vec<Range>) {
        if let Some((left, right)) = self.children.as_deref() {
            left.inner_ranges(ranges);
            right.inner_ranges(ranges);
        } else if self.children == Children::Data {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range.offset() {
                    last.length += self.range.length();
                    return;
                }
            }
            ranges.push(self.range);
        }
    }

    pub fn ranges(&self) -> Vec<Range> {
        let mut ranges = Vec::with_capacity(self.range.num_chunks() as _);
        self.inner_ranges(&mut ranges);
        ranges
    }

    fn inner_missing_ranges(&self, ranges: &mut Vec<Range>) {
        if let Some((left, right)) = self.children.as_deref() {
            left.inner_missing_ranges(ranges);
            right.inner_missing_ranges(ranges);
        } else if self.children == Children::None {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range.offset() {
                    last.length += self.range.length();
                    return;
                }
            }
            ranges.push(self.range);
        }
    }

    pub fn missing_ranges(&self) -> Vec<Range> {
        let mut ranges = Vec::with_capacity(self.range.num_chunks() as _);
        self.inner_missing_ranges(&mut ranges);
        ranges
    }

    fn inner_encode_range_to(
        &self,
        range: &Range,
        tree: &mut impl Write,
        chunks: &mut (impl Read + Seek),
    ) -> Result<()> {
        if self.is_chunk() {
            if range.intersects(self.range()) {
                if self.children == Children::Data {
                    let chunk = &mut [0; 1024][..self.range.length() as _];
                    chunks.seek(SeekFrom::Start(self.range.offset()))?;
                    chunks.read_exact(chunk)?;
                    tree.write_all(chunk)?;
                } else {
                    anyhow::bail!("missing chunk");
                }
            }
        } else if let Some((left, right)) = self.children.as_deref() {
            tree.write_all(left.hash().as_bytes())?;
            tree.write_all(right.hash().as_bytes())?;
            if range.intersects(left.range()) {
                left.inner_encode_range_to(range, tree, chunks)?;
            }
            if range.intersects(right.range()) {
                right.inner_encode_range_to(range, tree, chunks)?;
            }
        } else {
            anyhow::bail!("missing node");
        }
        Ok(())
    }

    pub fn encode_range_to(
        &self,
        range: &Range,
        tree: &mut impl Write,
        chunks: &mut (impl Read + Seek),
    ) -> Result<()> {
        anyhow::ensure!(self.is_root);
        let length = self.range().length();
        tree.write_all(&length.to_le_bytes()[..])?;
        self.inner_encode_range_to(range, tree, chunks)
    }

    pub fn encode_range(&self, range: &Range, chunks: &mut (impl Read + Seek)) -> Vec<u8> {
        let mut tree = Vec::with_capacity(range.encoded_size() as _);
        self.encode_range_to(range, &mut tree, chunks).unwrap();
        tree
    }

    pub fn encode(&self, chunks: &mut (impl Read + Seek)) -> Vec<u8> {
        self.encode_range(self.range(), chunks)
    }

    fn inner_decode_range_from(
        &mut self,
        range: Range,
        tree: &mut impl Read,
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        if self.is_chunk() {
            if self.children == Children::None && range.intersects(self.range()) {
                let chunk = &mut [0; 1024][..self.range.length() as _];
                tree.read_exact(chunk)?;
                let hash = blake3::guts::ChunkState::new(self.range.index())
                    .update(chunk)
                    .finalize(self.is_root);
                anyhow::ensure!(*self.hash() == hash);
                chunks.seek(SeekFrom::Start(self.range().offset()))?;
                chunks.write_all(chunk)?;
                self.children = Children::Data;
            }
        } else {
            let mut left_hash = [0; 32];
            tree.read_exact(&mut left_hash)?;
            let left_hash = Hash::from(left_hash);

            let mut right_hash = [0; 32];
            tree.read_exact(&mut right_hash)?;
            let right_hash = Hash::from(right_hash);

            let hash = blake3::guts::parent_cv(&left_hash, &right_hash, self.is_root);
            anyhow::ensure!(*self.hash() == hash);

            if self.children == Children::None {
                let (left_range, right_range) = self.range().split().unwrap();
                let left = Self {
                    is_root: false,
                    hash: left_hash,
                    range: left_range,
                    children: Children::None,
                };
                let right = Self {
                    is_root: false,
                    hash: right_hash,
                    range: right_range,
                    children: Children::None,
                };
                self.children = Children::Tree(Box::new((left, right)));
            }
            let (left, right) = self.children.as_deref_mut().unwrap();
            if range.intersects(left.range()) {
                left.inner_decode_range_from(range, tree, chunks)?;
            }
            if range.intersects(right.range()) {
                right.inner_decode_range_from(range, tree, chunks)?;
            }
        }
        Ok(())
    }

    pub fn decode_range_from(
        &mut self,
        range: Range,
        tree: &mut impl Read,
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        anyhow::ensure!(self.is_root);
        let mut length = [0; 8];
        tree.read_exact(&mut length)?;
        let length = u64::from_le_bytes(length);
        anyhow::ensure!(self.range == Range::new(0, length));
        self.inner_decode_range_from(range, tree, chunks)
    }

    pub fn decode_range(
        &mut self,
        range: Range,
        mut tree: &[u8],
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        self.decode_range_from(range, &mut tree, chunks)
    }

    pub fn decode(&mut self, tree: &[u8], chunks: &mut (impl Write + Seek)) -> Result<()> {
        self.decode_range(self.range, tree, chunks)
    }
}

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
        let mut right = Tree {
            range,
            is_root,
            hash,
            children: Children::Data,
        };
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
            right = Tree {
                range,
                is_root,
                hash,
                children: Children::Tree(Box::new((left, right))),
            };
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
            right = Tree {
                range,
                is_root,
                hash,
                children: Children::Tree(Box::new((left, right))),
            }
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
    use bao::encode::SliceExtractor;
    use std::fs::File;
    use std::io::{BufReader, BufWriter, Cursor};

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
    fn test_intersects() {
        let ranges = [
            ((0, 0), (0, 0)),
            ((2, 5), (1, 6)),
            ((2, 5), (3, 4)),
            ((2, 4), (3, 5)),
            ((3, 5), (2, 4)),
        ];
        for ((a, b), (c, d)) in ranges {
            let a = Range::new(a, b);
            let b = Range::new(c, d);
            assert!(a.intersects(&b));
        }
    }

    #[test]
    fn test_doesnt_intersect() {
        let ranges = [((0, 0), (1, 0)), ((0, 1), (2, 1)), ((2, 5), (0, 1))];
        for ((a, b), (c, d)) in ranges {
            let a = Range::new(a, b);
            let b = Range::new(c, d);
            assert!(!a.intersects(&b));
        }
    }

    #[test]
    fn test_tree_hasher() {
        let buf = [0x42; 65537];
        for &case in TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let hash = blake3::hash(bytes);
            let tree = tree_hash(bytes);
            assert_eq!(*tree.hash(), hash);
        }
    }

    #[test]
    fn test_tree() {
        let buf = [0x42; 65537];
        for &case in TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let mut buffer = vec![];
            let (bao_bytes, bao_hash) = bao::encode::encode(bytes);
            let tree = tree_hash(bytes);
            //dbg!(&tree);
            assert_eq!(tree.hash(), &bao_hash);
            assert!(tree.complete());
            assert_eq!(tree.length(), Some(case));
            assert_eq!(tree.ranges(), vec![tree.range]);
            assert_eq!(tree.missing_ranges(), vec![]);

            let mut tree2 = Tree::new(*tree.hash(), tree.range().length());
            assert_eq!(tree2.hash(), &bao_hash);
            assert!(!tree2.complete());
            assert_eq!(tree2.length(), None);
            assert_eq!(tree2.ranges(), vec![]);
            assert_eq!(tree2.missing_ranges(), vec![tree.range]);

            let slice = tree.encode(&mut Cursor::new(bytes));
            assert!(slice.len() as u64 <= tree.range().encoded_size());
            assert_eq!(bao_bytes, slice);
            buffer.clear();
            tree2.decode(&slice, &mut Cursor::new(&mut buffer)).unwrap();
            assert_eq!(tree2, tree);
            assert_eq!(bytes, buffer);

            if let Some((left_range, right_range)) = tree.range().split() {
                let left_slice = tree.encode_range(&left_range, &mut Cursor::new(bytes));
                let mut left_slice2 = vec![];
                SliceExtractor::new(
                    Cursor::new(&bao_bytes),
                    left_range.offset(),
                    left_range.length(),
                )
                .read_to_end(&mut left_slice2)
                .unwrap();
                assert_eq!(left_slice, left_slice2);

                let right_slice = tree.encode_range(&right_range, &mut Cursor::new(bytes));
                let mut right_slice2 = vec![];
                SliceExtractor::new(
                    Cursor::new(&bao_bytes),
                    right_range.offset(),
                    right_range.length(),
                )
                .read_to_end(&mut right_slice2)
                .unwrap();
                assert_eq!(right_slice, right_slice2);

                buffer.clear();
                let mut tree2 = Tree::new(*tree.hash(), tree.range().length());

                tree2
                    .decode_range(left_range, &left_slice, &mut Cursor::new(&mut buffer))
                    .unwrap();
                assert_eq!(tree2.hash(), &bao_hash);
                assert!(!tree2.complete());
                assert_eq!(tree2.length(), None);
                assert_eq!(tree2.ranges(), vec![left_range]);
                assert_eq!(tree2.missing_ranges(), vec![right_range]);

                tree2
                    .decode_range(right_range, &right_slice, &mut Cursor::new(&mut buffer))
                    .unwrap();
                assert_eq!(tree2, tree);
                assert_eq!(bytes, buffer);
            }
        }
    }

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
