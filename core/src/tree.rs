use crate::{Hash, Range, Result, StreamId};
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Insertion {
    Parent(Hash, Hash, Hash),
    Chunk(Hash),
}

#[derive(Clone, Debug)]
pub struct Tree {
    tree: sled::Tree,
    id: StreamId,
    hash: Hash,
    range: Range,
    is_root: bool,
}

impl PartialEq for Tree {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Tree {
    pub fn open(db: &sled::Db, id: StreamId) -> Result<Self> {
        let tree = db.open_tree(id.to_bytes())?;
        Ok(Self {
            tree,
            id,
            hash: *id.hash(),
            range: id.range(),
            is_root: true,
        })
    }

    fn insert(&self, insertion: &Insertion) -> Result<()> {
        match insertion {
            Insertion::Chunk(hash) => {
                self.tree.insert(hash.as_bytes(), &[])?;
            }
            Insertion::Parent(hash, left, right) => {
                let mut value = [0; 64];
                value[..32].copy_from_slice(left.as_bytes());
                value[32..].copy_from_slice(right.as_bytes());
                self.tree.insert(hash.as_bytes(), &value[..])?;
            }
        }
        Ok(())
    }

    pub(crate) fn apply_batch(&self, batch: &[Insertion]) -> Result<()> {
        for insertion in batch {
            self.insert(insertion)?;
        }
        Ok(())
    }

    pub fn id(&self) -> &StreamId {
        &self.id
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

    fn is_missing(&self) -> Result<bool> {
        Ok(!self.tree.contains_key(self.hash().as_bytes())?)
    }

    fn data(&self) -> Result<bool> {
        Ok(self.is_chunk() && self.tree.contains_key(self.hash().as_bytes())?)
    }

    fn set_data(&self) -> Result<()> {
        self.insert(&Insertion::Chunk(*self.hash()))
    }

    fn children(&self) -> Result<Option<(Self, Self)>> {
        Ok(self.tree.get(self.hash.as_bytes())?.and_then(|bytes| {
            if bytes.is_empty() {
                return None;
            }
            let (left, right) = bytes.split_at(32);
            let mut hash = [0; 32];
            hash.copy_from_slice(left);
            let left = Hash::from(hash);
            hash.copy_from_slice(right);
            let right = Hash::from(hash);
            let range = self.range.split().unwrap();
            let left = Self {
                tree: self.tree.clone(),
                id: self.id,
                hash: left,
                range: range.0,
                is_root: false,
            };
            let right = Self {
                tree: self.tree.clone(),
                id: self.id,
                hash: right,
                range: range.1,
                is_root: false,
            };
            Some((left, right))
        }))
    }

    fn set_children(&self, left: &Hash, right: &Hash) -> Result<()> {
        self.insert(&Insertion::Parent(*self.hash(), *left, *right))
    }

    fn last_chunk(&self) -> Result<Tree> {
        Ok(if let Some((_, right)) = self.children()? {
            right.last_chunk()?
        } else {
            self.clone()
        })
    }

    pub fn length(&self) -> Result<Option<u64>> {
        let last = self.last_chunk()?;
        Ok(if last.data()? {
            Some(last.range().end())
        } else {
            None
        })
    }

    pub fn complete(&self) -> Result<bool> {
        self.has_range(self.range())
    }

    pub fn has_range(&self, range: &Range) -> Result<bool> {
        if self.is_missing()? && range.intersects(self.range()) {
            return Ok(false);
        }
        if let Some((left, right)) = self.children()? {
            return Ok(left.has_range(range)? && right.has_range(range)?);
        }
        Ok(true)
    }

    fn inner_ranges(&self, ranges: &mut Vec<Range>) -> Result<()> {
        if let Some((left, right)) = self.children()? {
            left.inner_ranges(ranges)?;
            right.inner_ranges(ranges)?;
        } else if self.data()? {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range().offset() {
                    last.extend(self.range().length());
                    return Ok(());
                }
            }
            ranges.push(*self.range());
        }
        Ok(())
    }

    pub fn ranges(&self) -> Result<Vec<Range>> {
        let mut ranges = Vec::with_capacity(self.range().num_chunks() as _);
        self.inner_ranges(&mut ranges)?;
        Ok(ranges)
    }

    fn inner_missing_ranges(&self, ranges: &mut Vec<Range>) -> Result<()> {
        if let Some((left, right)) = self.children()? {
            left.inner_missing_ranges(ranges)?;
            right.inner_missing_ranges(ranges)?;
        } else if self.is_missing()? {
            if let Some(last) = ranges.last_mut() {
                if last.end() == self.range().offset() {
                    last.extend(self.range().length());
                    return Ok(());
                }
            }
            ranges.push(*self.range());
        }
        Ok(())
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
                if self.data()? {
                    let chunk = &mut [0; 1024][..self.range().length() as _];
                    chunks.seek(SeekFrom::Start(self.range().offset()))?;
                    chunks.read_exact(chunk)?;
                    tree.write_all(chunk)?;
                } else {
                    anyhow::bail!("missing chunk");
                }
            }
        } else if let Some((left, right)) = self.children()? {
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
        buffer: &mut [u8; 1024],
    ) -> Result<()> {
        if self.is_chunk() {
            if self.is_missing()? && range.intersects(self.range()) {
                let chunk = &mut buffer[..self.range().length() as _];
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

            self.set_children(&left_hash, &right_hash)?;
            let (left, right) = self.children()?.unwrap();
            if range.intersects(left.range()) {
                left.inner_decode_range_from(range, tree, chunks, buffer)?;
            }
            if range.intersects(right.range()) {
                right.inner_decode_range_from(range, tree, chunks, buffer)?;
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
        let mut buffer = [0; 1024];
        self.inner_decode_range_from(range, tree, chunks, &mut buffer)
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
    use crate::{tree_hash, Mime};
    use bao::encode::SliceExtractor;
    use std::io::Cursor;

    #[test]
    fn test_tree() -> Result<()> {
        let buf = [0x42; 65537];
        let db0 = crate::tests::memory(0)?;
        let db1 = crate::tests::memory(1)?;
        let db2 = crate::tests::memory(2)?;
        for &case in crate::tests::TEST_CASES {
            dbg!(case);
            let bytes = &buf[..(case as _)];
            let mut buffer = vec![];
            let (bao_bytes, bao_hash) = bao::encode::encode(bytes);
            let tree = tree_hash(&db0, bytes, Mime::ApplicationOctetStream)?;
            let id = *tree.id();

            //dbg!(&tree);
            assert_eq!(tree.hash(), &bao_hash);
            assert!(tree.complete()?);
            assert_eq!(tree.length()?, Some(case));
            assert_eq!(tree.ranges()?, vec![*tree.range()]);
            assert_eq!(tree.missing_ranges()?, vec![]);

            let tree2 = Tree::open(&db1, id)?;
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
                let tree2 = Tree::open(&db2, id)?;

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
