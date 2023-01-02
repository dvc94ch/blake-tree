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
        !(self.end() < other.offset() || self.offset() > other.end())
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}..{}", self.offset, self.end())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    range: Range,
    is_root: bool,
    hash: Hash,
    children: Option<(Box<Node>, Box<Node>)>,
}

impl Node {
    pub fn new(bytes: &[u8]) -> Self {
        let range = Range::new(bytes.len() as _);
        Self::inner_new(range, true, bytes)
    }

    fn inner_new(range: Range, is_root: bool, bytes: &[u8]) -> Self {
        debug_assert_eq!(range.length(), bytes.len() as u64);
        if let Some((left_range, right_range)) = range.split() {
            let at = left_range.end() - left_range.offset();
            let (left_bytes, right_bytes) = bytes.split_at(at as _);
            let left = Box::new(Node::inner_new(left_range, false, left_bytes));
            let right = Box::new(Node::inner_new(right_range, false, right_bytes));
            let hash = blake3::guts::parent_cv(left.hash(), right.hash(), is_root);
            Self {
                range,
                is_root,
                hash,
                children: Some((left, right)),
            }
        } else {
            let hash = blake3::guts::ChunkState::new(range.index())
                .update(bytes)
                .finalize(is_root);
            Self {
                range,
                is_root,
                hash,
                children: None,
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
        self.children.as_ref().map(|(left, _)| &**left)
    }

    pub fn right(&self) -> Option<&Node> {
        self.children.as_ref().map(|(_, right)| &**right)
    }

    pub fn complete(&self) -> bool {
        if self.is_chunk() {
            true
        } else if let Some((left, right)) = self.children.as_ref() {
            left.complete() && right.complete()
        } else {
            false
        }
    }

    pub fn length(&self) -> u64 {
        self.range.length()
    }

    pub fn extract(&self, range: &Range) -> Option<Self> {
        let mut node = Node {
            range: self.range,
            is_root: self.is_root,
            hash: self.hash,
            children: None,
        };
        if self.is_chunk() {
            return Some(node);
        }
        let (left, right) = self.children.as_ref()?;
        if self.range.intersects(range) {
            let left = left.extract(range)?;
            let right = right.extract(range)?;
            node.children = Some((Box::new(left), Box::new(right)));
        }
        Some(node)
    }

    pub fn verify(&mut self, _root: &Hash, _other: &Self) -> Result<()> {
        todo!()
    }

    pub fn encode(&self) -> Vec<u8> {
        todo!()
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self> {
        todo!()
    }

    pub fn ranges(&self) -> Vec<Range> {
        todo!()
    }

    pub fn missing_ranges(&self, _length: u64) -> Vec<Range> {
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
            let input = &buf[..(case as _)];
            let expected = blake3::hash(input);
            let tree = Node::new(input);
            dbg!(&tree);
            assert_eq!(tree.hash(), &expected);
            assert!(tree.complete());
            assert_eq!(tree.length(), case);
        }
    }
}
