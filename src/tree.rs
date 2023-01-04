use crate::{Hash, Range, Result};
use std::io::{Read, Seek, SeekFrom, Write};

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
    hash: Hash,
    range: Range,
    is_root: bool,
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

    pub(crate) fn chunk(hash: Hash, range: Range, is_root: bool) -> Self {
        Self {
            hash,
            range,
            is_root,
            children: Children::Data,
        }
    }

    pub(crate) fn subtree(hash: Hash, range: Range, is_root: bool, left: Tree, right: Tree) -> Self {
        Self {
            hash,
            range,
            is_root,
            children: Children::Tree(Box::new((left, right))),
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

    pub fn has_range(&self, range: &Range) -> bool {
        if self.children == Children::None && range.intersects(self.range()) {
            return false;
        }
        if let Some((left, right)) = self.children.as_deref() {
            return left.has_range(range) && right.has_range(range);
        }
        true
    }

    fn inner_ranges(&self, ranges: &mut Vec<Range>) {
        if let Some((left, right)) = self.children.as_deref() {
            left.inner_ranges(ranges);
            right.inner_ranges(ranges);
        } else if self.children == Children::Data {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range.offset() {
                    last.extend(self.range.length());
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
                    last.extend(self.range.length());
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

#[cfg(test)]
mod tests {
    use super::*;
    use bao::encode::SliceExtractor;
    use std::io::Cursor;

    #[test]
    fn test_tree() {
        let buf = [0x42; 65537];
        for &case in crate::tests::TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let mut buffer = vec![];
            let (bao_bytes, bao_hash) = bao::encode::encode(bytes);
            let tree = crate::tree_hash(bytes);
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
}
