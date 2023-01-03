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
    pub fn new(length: u64) -> Self {
        Self { offset: 0, length }
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
            let first = Range {
                offset: self.offset,
                length: at,
            };
            let second = Range {
                offset: self.offset + at,
                length: self.length - at,
            };
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
    Tree(Box<(Node, Node)>),
}

impl Children {
    fn is_none(&self) -> bool {
        self == &Self::None
    }

    fn as_deref(&self) -> Option<&(Node, Node)> {
        match self {
            Self::Tree(nodes) => Some(&**nodes),
            _ => None,
        }
    }

    fn as_deref_mut(&mut self) -> Option<&mut (Node, Node)> {
        match self {
            Self::Tree(nodes) => Some(&mut **nodes),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    range: Range,
    is_root: bool,
    hash: Hash,
    children: Children,
}

impl Node {
    pub fn root(hash: Hash, length: u64) -> Self {
        Self {
            range: Range::new(length),
            is_root: true,
            hash,
            children: Children::None,
        }
    }

    pub fn new(bytes: &[u8]) -> Self {
        let range = Range::new(bytes.len() as _);
        Self::inner_new(range, true, bytes)
    }

    fn inner_new(range: Range, is_root: bool, bytes: &[u8]) -> Self {
        debug_assert_eq!(range.length(), bytes.len() as u64);
        if let Some((left_range, right_range)) = range.split() {
            let at = left_range.end() - left_range.offset();
            let (left_bytes, right_bytes) = bytes.split_at(at as _);
            let left = Node::inner_new(left_range, false, left_bytes);
            let right = Node::inner_new(right_range, false, right_bytes);
            let hash = blake3::guts::parent_cv(left.hash(), right.hash(), is_root);
            Self {
                range,
                is_root,
                hash,
                children: Children::Tree(Box::new((left, right))),
            }
        } else {
            let hash = blake3::guts::ChunkState::new(range.index())
                .update(bytes)
                .finalize(is_root);
            Self {
                range,
                is_root,
                hash,
                children: Children::Data,
            }
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

    pub fn left(&self) -> Option<&Node> {
        self.children.as_deref().map(|(left, _)| left)
    }

    pub fn right(&self) -> Option<&Node> {
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

    pub fn last_chunk(&self) -> &Node {
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
        anyhow::ensure!(self.range == Range::new(length));
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

#[cfg(test)]
mod tests {
    use super::*;
    use bao::encode::SliceExtractor;
    use std::io::Cursor;

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
            let a = Range {
                offset: a,
                length: b,
            };
            let b = Range {
                offset: c,
                length: d,
            };
            assert!(a.intersects(&b));
        }
    }

    #[test]
    fn test_doesnt_intersect() {
        let ranges = [((0, 0), (1, 0)), ((0, 1), (2, 1)), ((2, 5), (0, 1))];
        for ((a, b), (c, d)) in ranges {
            let a = Range {
                offset: a,
                length: b,
            };
            let b = Range {
                offset: c,
                length: d,
            };
            assert!(!a.intersects(&b));
        }
    }

    #[test]
    fn test_hash_matches() {
        let buf = [0x42; 65537];
        for &case in TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let mut buffer = vec![];
            let (bao_bytes, bao_hash) = bao::encode::encode(bytes);
            let tree = Node::new(bytes);
            //dbg!(&tree);
            assert_eq!(tree.hash(), &bao_hash);
            assert!(tree.complete());
            assert_eq!(tree.length(), Some(case));
            assert_eq!(tree.ranges(), vec![tree.range]);
            assert_eq!(tree.missing_ranges(), vec![]);

            let mut tree2 = Node::root(*tree.hash(), tree.range().length());
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
                let mut tree2 = Node::root(*tree.hash(), tree.range().length());

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
}
