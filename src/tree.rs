use crate::{Hash, Range, Result};
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Clone, Debug)]
pub struct NodeStorage {
    tree: sled::Tree,
}

impl NodeStorage {
    pub fn new(tree: sled::Tree) -> Self {
        Self { tree }
    }

    #[cfg(test)]
    pub(crate) fn memory() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        let tree = db.open_tree("trees")?;
        Ok(Self::new(tree))
    }

    pub(crate) fn get(&self, _hash: &Hash) -> Result<Option<Tree>> {
        todo!()
    }

    fn insert(&self, _tree: &Node) -> Result<()> {
        todo!()
    }

    fn insert_children(&self, _hash: &Hash, _left: &Node, _right: &Node) -> Result<()> {
        todo!()
    }

    fn set_data(&self, _hash: &Hash) -> Result<()> {
        todo!()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Children {
    None,
    Data,
    Tree(Hash, Hash),
}

impl Children {
    fn as_chunk(&self) -> Option<()> {
        if *self == Self::Data {
            Some(())
        } else {
            None
        }
    }

    fn as_subtree(&self) -> Option<(&Hash, &Hash)> {
        if let Self::Tree(left, right) = self {
            Some((left, right))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Node {
    hash: Hash,
    range: Range,
    is_root: bool,
    children: Children,
}

impl Node {
    fn new(hash: Hash, length: u64) -> Self {
        Self {
            range: Range::new(0, length),
            is_root: true,
            hash,
            children: Children::None,
        }
    }

    fn chunk(hash: Hash, range: Range, is_root: bool) -> Self {
        Self {
            hash,
            range,
            is_root,
            children: Children::Data,
        }
    }

    fn subtree(hash: Hash, range: Range, is_root: bool, left: Hash, right: Hash) -> Self {
        Self {
            hash,
            range,
            is_root,
            children: Children::Tree(left, right),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tree {
    storage: NodeStorage,
    node: Node,
}

impl PartialEq for Tree {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl Tree {
    fn create_node(storage: NodeStorage, node: Node) -> Result<Self> {
        storage.insert(&node)?;
        Ok(Self { storage, node })
    }

    pub fn new(storage: NodeStorage, hash: Hash, length: u64) -> Result<Self> {
        Self::create_node(storage, Node::new(hash, length))
    }

    pub(crate) fn chunk(
        storage: NodeStorage,
        hash: Hash,
        range: Range,
        is_root: bool,
    ) -> Result<Self> {
        Self::create_node(storage, Node::chunk(hash, range, is_root))
    }

    pub(crate) fn subtree(
        storage: NodeStorage,
        hash: Hash,
        range: Range,
        is_root: bool,
        left: Hash,
        right: Hash,
    ) -> Result<Self> {
        Self::create_node(storage, Node::subtree(hash, range, is_root, left, right))
    }

    pub fn hash(&self) -> &Hash {
        &self.node.hash
    }

    pub fn range(&self) -> &Range {
        &self.node.range
    }

    pub fn is_root(&self) -> bool {
        self.node.is_root
    }

    pub fn is_chunk(&self) -> bool {
        self.node.range.is_chunk()
    }

    pub fn is_missing(&self) -> bool {
        self.node.children == Children::None
    }

    pub fn as_chunk(&self) -> Option<()> {
        self.node.children.as_chunk()
    }

    pub fn as_subtree(&self) -> Option<(&Hash, &Hash)> {
        self.node.children.as_subtree()
    }

    fn set_data(&self) -> Result<()> {
        self.storage.set_data(self.hash())
    }

    fn create_children(&self, left: Hash, right: Hash) -> Result<(Tree, Tree)> {
        let (left_range, right_range) = self.range().split().unwrap();
        let left = Self {
            node: Node {
                is_root: false,
                hash: left,
                range: left_range,
                children: Children::None,
            },
            storage: self.storage.clone(),
        };
        let right = Self {
            node: Node {
                is_root: false,
                hash: right,
                range: right_range,
                children: Children::None,
            },
            storage: self.storage.clone(),
        };
        self.storage
            .insert_children(self.hash(), &left.node, &right.node)?;
        Ok((left, right))
    }

    pub fn left(&self) -> Result<Option<Tree>> {
        Ok(if let Some((left, _)) = self.as_subtree() {
            self.storage.get(left)?
        } else {
            None
        })
    }

    pub fn right(&self) -> Result<Option<Tree>> {
        Ok(if let Some((_, right)) = self.as_subtree() {
            self.storage.get(right)?
        } else {
            None
        })
    }

    pub fn last_chunk(&self) -> Result<Tree> {
        Ok(if let Some(right) = self.right()? {
            right.last_chunk()?
        } else {
            self.clone()
        })
    }

    pub fn length(&self) -> Result<Option<u64>> {
        let last = self.last_chunk()?;
        Ok(if last.as_chunk().is_some() {
            Some(last.range().end())
        } else {
            None
        })
    }

    pub fn complete(&self) -> Result<bool> {
        self.has_range(self.range())
    }

    pub fn has_range(&self, range: &Range) -> Result<bool> {
        if self.is_missing() && range.intersects(self.range()) {
            return Ok(false);
        }
        if let Some((left, right)) = self.as_subtree() {
            let left = self.storage.get(left)?.unwrap().has_range(range)?;
            let right = self.storage.get(right)?.unwrap().has_range(range)?;
            return Ok(left && right);
        }
        Ok(true)
    }

    fn inner_ranges(&self, ranges: &mut Vec<Range>) -> Result<()> {
        Ok(if let Some((left, right)) = self.as_subtree() {
            let left = self.storage.get(left)?.unwrap();
            let right = self.storage.get(right)?.unwrap();
            left.inner_ranges(ranges)?;
            right.inner_ranges(ranges)?;
        } else if self.as_chunk().is_some() {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range().offset() {
                    last.extend(self.range().length());
                    return Ok(());
                }
            }
            ranges.push(*self.range());
        })
    }

    pub fn ranges(&self) -> Result<Vec<Range>> {
        let mut ranges = Vec::with_capacity(self.range().num_chunks() as _);
        self.inner_ranges(&mut ranges)?;
        Ok(ranges)
    }

    fn inner_missing_ranges(&self, ranges: &mut Vec<Range>) -> Result<()> {
        Ok(if let Some((left, right)) = self.as_subtree() {
            let left = self.storage.get(left)?.unwrap();
            let right = self.storage.get(right)?.unwrap();
            left.inner_missing_ranges(ranges)?;
            right.inner_missing_ranges(ranges)?;
        } else if self.is_missing() {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range().offset() {
                    last.extend(self.range().length());
                    return Ok(());
                }
            }
            ranges.push(*self.range());
        })
    }

    pub fn missing_ranges(&self) -> Result<Vec<Range>> {
        let mut ranges = Vec::with_capacity(self.range().num_chunks() as _);
        self.inner_missing_ranges(&mut ranges)?;
        Ok(ranges)
    }

    fn inner_encode_range_to(
        &self,
        range: &Range,
        tree: &mut impl Write,
        chunks: &mut (impl Read + Seek),
    ) -> Result<()> {
        if self.is_chunk() {
            if range.intersects(self.range()) {
                if self.as_chunk().is_some() {
                    let chunk = &mut [0; 1024][..self.range().length() as _];
                    chunks.seek(SeekFrom::Start(self.range().offset()))?;
                    chunks.read_exact(chunk)?;
                    tree.write_all(chunk)?;
                } else {
                    anyhow::bail!("missing chunk");
                }
            }
        } else if let Some((left, right)) = self.as_subtree() {
            tree.write_all(left.as_bytes())?;
            tree.write_all(right.as_bytes())?;
            let left = self.storage.get(left)?.unwrap();
            if range.intersects(left.range()) {
                left.inner_encode_range_to(range, tree, chunks)?;
            }
            let right = self.storage.get(right)?.unwrap();
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
        anyhow::ensure!(self.is_root());
        let length = self.range().length();
        tree.write_all(&length.to_le_bytes()[..])?;
        self.inner_encode_range_to(range, tree, chunks)
    }

    pub fn encode_range(&self, range: &Range, chunks: &mut (impl Read + Seek)) -> Result<Vec<u8>> {
        let mut tree = Vec::with_capacity(range.encoded_size() as _);
        self.encode_range_to(range, &mut tree, chunks)?;
        Ok(tree)
    }

    pub fn encode(&self, chunks: &mut (impl Read + Seek)) -> Result<Vec<u8>> {
        self.encode_range(self.range(), chunks)
    }

    fn inner_decode_range_from(
        &self,
        range: &Range,
        tree: &mut impl Read,
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        if self.is_chunk() {
            if self.is_missing() && range.intersects(self.range()) {
                let chunk = &mut [0; 1024][..self.range().length() as _];
                tree.read_exact(chunk)?;
                let hash = blake3::guts::ChunkState::new(self.range().index())
                    .update(chunk)
                    .finalize(self.is_root());
                anyhow::ensure!(*self.hash() == hash);
                chunks.seek(SeekFrom::Start(self.range().offset()))?;
                chunks.write_all(chunk)?;
                self.set_data()?;
            }
        } else {
            let mut left_hash = [0; 32];
            tree.read_exact(&mut left_hash)?;
            let left_hash = Hash::from(left_hash);

            let mut right_hash = [0; 32];
            tree.read_exact(&mut right_hash)?;
            let right_hash = Hash::from(right_hash);

            let hash = blake3::guts::parent_cv(&left_hash, &right_hash, self.is_root());
            anyhow::ensure!(*self.hash() == hash);

            let (left, right) = if let Some((left, right)) = self.as_subtree() {
                let left = self.storage.get(left)?.unwrap();
                let right = self.storage.get(right)?.unwrap();
                (left, right)
            } else {
                self.create_children(left_hash, right_hash)?
            };
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
        &self,
        range: &Range,
        tree: &mut impl Read,
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        anyhow::ensure!(self.is_root());
        let mut length = [0; 8];
        tree.read_exact(&mut length)?;
        let length = u64::from_le_bytes(length);
        anyhow::ensure!(*self.range() == Range::new(0, length));
        self.inner_decode_range_from(range, tree, chunks)
    }

    pub fn decode_range(
        &self,
        range: &Range,
        mut tree: &[u8],
        chunks: &mut (impl Write + Seek),
    ) -> Result<()> {
        self.decode_range_from(range, &mut tree, chunks)
    }

    pub fn decode(&self, tree: &[u8], chunks: &mut (impl Write + Seek)) -> Result<()> {
        self.decode_range(self.range(), tree, chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bao::encode::SliceExtractor;
    use std::io::Cursor;

    #[test]
    fn test_tree() -> Result<()> {
        let s1 = NodeStorage::memory()?;
        let s2 = NodeStorage::memory()?;
        let s3 = NodeStorage::memory()?;
        let buf = [0x42; 65537];
        for &case in crate::tests::TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let mut buffer = vec![];
            let (bao_bytes, bao_hash) = bao::encode::encode(bytes);
            let tree = crate::tree_hash(s1.clone(), bytes)?;
            //dbg!(&tree);
            assert_eq!(tree.hash(), &bao_hash);
            assert!(tree.complete()?);
            assert_eq!(tree.length()?, Some(case));
            assert_eq!(tree.ranges()?, vec![*tree.range()]);
            assert_eq!(tree.missing_ranges()?, vec![]);

            let tree2 = Tree::new(s2.clone(), *tree.hash(), tree.range().length())?;
            assert_eq!(tree2.hash(), &bao_hash);
            assert!(!tree2.complete()?);
            assert_eq!(tree2.length()?, None);
            assert_eq!(tree2.ranges()?, vec![]);
            assert_eq!(tree2.missing_ranges()?, vec![*tree.range()]);

            let slice = tree.encode(&mut Cursor::new(bytes))?;
            assert!(slice.len() as u64 <= tree.range().encoded_size());
            assert_eq!(bao_bytes, slice);
            buffer.clear();
            tree2.decode(&slice, &mut Cursor::new(&mut buffer))?;
            assert_eq!(tree2, tree);
            assert_eq!(bytes, buffer);

            if let Some((left_range, right_range)) = tree.range().split() {
                let left_slice = tree.encode_range(&left_range, &mut Cursor::new(bytes))?;
                let mut left_slice2 = vec![];
                SliceExtractor::new(
                    Cursor::new(&bao_bytes),
                    left_range.offset(),
                    left_range.length(),
                )
                .read_to_end(&mut left_slice2)?;
                assert_eq!(left_slice, left_slice2);

                let right_slice = tree.encode_range(&right_range, &mut Cursor::new(bytes))?;
                let mut right_slice2 = vec![];
                SliceExtractor::new(
                    Cursor::new(&bao_bytes),
                    right_range.offset(),
                    right_range.length(),
                )
                .read_to_end(&mut right_slice2)?;
                assert_eq!(right_slice, right_slice2);

                buffer.clear();
                let tree2 = Tree::new(s3.clone(), *tree.hash(), tree.range().length())?;

                tree2.decode_range(&left_range, &left_slice, &mut Cursor::new(&mut buffer))?;
                assert_eq!(tree2.hash(), &bao_hash);
                assert!(!tree2.complete()?);
                assert_eq!(tree2.length()?, None);
                assert_eq!(tree2.ranges()?, vec![left_range]);
                assert_eq!(tree2.missing_ranges()?, vec![right_range]);

                tree2.decode_range(&right_range, &right_slice, &mut Cursor::new(&mut buffer))?;
                assert_eq!(tree2, tree);
                assert_eq!(bytes, buffer);
            }
        }
        Ok(())
    }
}
