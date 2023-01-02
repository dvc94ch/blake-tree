use anyhow::Result;
use blake3::Hash;

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
        !(self.end() <= other.offset() || self.offset() >= other.end())
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

    pub fn extract(&self, range: &Range) -> Option<Self> {
        let mut node = Node {
            range: self.range,
            is_root: self.is_root,
            hash: self.hash,
            children: Children::None,
        };
        let intersects = self.range.intersects(range);
        if self.is_chunk() {
            if intersects {
                node.children = Children::Data;
            }
            return Some(node);
        }
        let (left, right) = self.children.as_deref()?;
        if intersects {
            let left = left.extract(range)?;
            let right = right.extract(range)?;
            node.children = Children::Tree(Box::new((left, right)));
        }
        Some(node)
    }

    fn inner_verify(&mut self, other: &Self, offset: u64, bytes: &[u8]) -> Result<()> {
        anyhow::ensure!(self.is_root() == other.is_root());
        anyhow::ensure!(self.range() == other.range());
        anyhow::ensure!(self.hash() == other.hash());
        if self.is_chunk() {
            if self.children == Children::None && other.children != Children::None {
                let start = (self.range().offset() - offset) as usize;
                let end = (self.range().end() - offset) as usize;
                anyhow::ensure!(bytes.len() >= start);
                anyhow::ensure!(bytes.len() >= end);
                let hash = blake3::guts::ChunkState::new(self.range().index())
                    .update(&bytes[start..end])
                    .finalize(self.is_root());
                anyhow::ensure!(self.hash() == &hash);
                self.children = Children::Data;
            }
        } else {
            let (left_range, right_range) = self.range().split().unwrap();
            let (other_left, other_right) = if let Some((left, right)) = other.children.as_deref() {
                (left, right)
            } else {
                return Ok(());
            };

            if self.children == Children::None {
                let hash =
                    blake3::guts::parent_cv(other_left.hash(), other_right.hash(), self.is_root());
                anyhow::ensure!(self.hash() == &hash);

                let left = Node {
                    range: left_range,
                    is_root: false,
                    hash: *other_left.hash(),
                    children: Children::None,
                };
                let right = Node {
                    range: right_range,
                    is_root: false,
                    hash: *other_right.hash(),
                    children: Children::None,
                };
                self.children = Children::Tree(Box::new((left, right)));
            }

            let (left, right) = self.children.as_deref_mut().unwrap();
            left.inner_verify(other_left, offset, bytes)?;
            right.inner_verify(other_right, offset, bytes)?;
        }
        Ok(())
    }

    pub fn verify(&mut self, other: &Self, bytes: &[u8]) -> Result<()> {
        self.inner_verify(other, other.ranges()[0].offset(), bytes)
    }

    fn inner_ranges(&self, ranges: &mut Vec<Range>) {
        if self.complete() {
            ranges.push(self.range);
        } else if let Some((left, right)) = self.children.as_deref() {
            left.inner_ranges(ranges);
            right.inner_ranges(ranges);
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
            ranges.push(self.range);
        }
    }

    pub fn missing_ranges(&self) -> Vec<Range> {
        let mut ranges = Vec::with_capacity(self.range.num_chunks() as _);
        self.inner_missing_ranges(&mut ranges);
        ranges
    }

    pub fn encode(&self) -> Vec<u8> {
        todo!()
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self> {
        todo!()
    }
}

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

    #[test]
    fn test_hash_matches() {
        let buf = [0x42; 65537];
        for &case in TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let expected = blake3::hash(bytes);
            let tree = Node::new(bytes);
            //dbg!(&tree);
            assert_eq!(tree.hash(), &expected);
            assert!(tree.complete());
            assert_eq!(tree.length(), Some(case));
            assert_eq!(tree.ranges(), vec![tree.range]);
            assert_eq!(tree.missing_ranges(), vec![]);

            let mut tree2 = Node::root(*tree.hash(), tree.range().length());
            assert_eq!(tree2.hash(), &expected);
            assert!(!tree2.complete());
            assert_eq!(tree2.length(), None);
            assert_eq!(tree2.ranges(), vec![]);
            assert_eq!(tree2.missing_ranges(), vec![tree.range]);

            tree2.verify(&tree, bytes).unwrap();
            assert_eq!(tree2, tree);

            if let Some((left, right)) = tree.range().split() {
                let (left_bytes, right_bytes) = buf.split_at(left.end() as _);

                dbg!(left);
                let left_tree = tree.extract(&left).unwrap();
                dbg!(&left_tree);
                assert_eq!(left_tree.hash(), &expected);
                assert!(!left_tree.complete());
                assert_eq!(left_tree.length(), None);
                assert_eq!(left_tree.ranges(), vec![left]);
                assert_eq!(left_tree.missing_ranges(), vec![right]);

                dbg!(right);
                let right_tree = tree.extract(&right).unwrap();
                dbg!(&right_tree);
                assert_eq!(right_tree.hash(), &expected);
                assert!(!right_tree.complete());
                assert_eq!(right_tree.length(), Some(case));
                assert_eq!(right_tree.ranges(), vec![right]);
                assert_eq!(right_tree.missing_ranges(), vec![left]);

                let mut tree2 = Node::root(*tree.hash(), tree.range().length());
                tree2.verify(&left_tree, left_bytes).unwrap();
                assert_eq!(tree2, left_tree);
                tree2.verify(&right_tree, right_bytes).unwrap();
                assert_eq!(tree2, tree);
            }
        }
    }
}
