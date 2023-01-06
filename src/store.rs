use crate::{NodeStorage, Range, Result, StreamId, Tree, TreeHasher};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub struct RangeReader {
    chunks: BufReader<File>,
    tree: Tree,
    range: Range,
}

impl RangeReader {
    fn new(path: &Path, tree: Tree, range: Range) -> Result<Self> {
        anyhow::ensure!(tree.has_range(&range)?);
        let chunks = BufReader::new(File::open(path)?);
        Ok(Self {
            chunks,
            tree,
            range,
        })
    }

    pub fn set_range(&mut self, range: Range) -> Result<()> {
        anyhow::ensure!(self.tree.has_range(&range)?);
        self.range = range;
        Ok(())
    }
}

impl Read for RangeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.stream_position()?;
        let rest = self.range.end() - pos;
        let n = u64::min(rest, buf.len() as _) as usize;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, ""));
        }
        self.chunks.read(&mut buf[..n])
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = self.stream_position()?;
        let current_pos = self.chunks.seek(from)?;
        if current_pos < self.range.offset() || current_pos >= self.range.end() {
            self.chunks.seek(SeekFrom::Start(pos))?;
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, ""));
        }
        Ok(current_pos)
    }
}

#[derive(Clone, Debug)]
pub struct Stream {
    id: StreamId,
    tree: Tree,
    path: PathBuf,
}

impl Stream {
    pub fn id(&self) -> StreamId {
        self.id
    }

    pub fn has_range(&self, range: &Range) -> Result<bool> {
        self.tree.has_range(range)
    }

    pub fn ranges(&self) -> Result<Vec<Range>> {
        self.tree.ranges()
    }

    pub fn missing_ranges(&self) -> Result<Vec<Range>> {
        self.tree.missing_ranges()
    }

    pub fn encode_range_to(&self, range: &Range, to: &mut impl Write) -> Result<()> {
        let mut chunks = BufReader::new(File::open(&self.path)?);
        self.tree.encode_range_to(range, to, &mut chunks)
    }

    pub fn encode_range(&self, range: &Range) -> Result<Vec<u8>> {
        let mut chunks = BufReader::new(File::open(&self.path)?);
        self.tree.encode_range(range, &mut chunks)
    }

    pub fn decode_range_from(&self, range: &Range, from: &mut impl Read) -> Result<()> {
        let mut chunks = BufWriter::new(File::open(&self.path)?);
        self.tree.decode_range_from(range, from, &mut chunks)?;
        chunks.flush()?;
        Ok(())
    }

    pub fn decode_range(&self, range: &Range, slice: &[u8]) -> Result<()> {
        let mut chunks = BufWriter::new(File::open(&self.path)?);
        self.tree.decode_range(range, slice, &mut chunks)?;
        chunks.flush()?;
        Ok(())
    }

    pub fn read_range(&self, range: Range) -> Result<RangeReader> {
        RangeReader::new(&self.path, self.tree.clone(), range)
    }

    pub fn read(&self) -> Result<RangeReader> {
        self.read_range(*self.tree.range())
    }
}

pub struct StreamStorage {
    chunks: PathBuf,
    db: sled::Db,
}

impl StreamStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let chunks = path.as_ref().join("chunks");
        let db = path.as_ref().join("trees");
        std::fs::create_dir_all(&chunks)?;
        let db = sled::open(db)?;
        // TODO: crash recovery
        Ok(Self { chunks, db })
    }

    pub fn streams(&self) -> impl Iterator<Item = StreamId> {
        self.db.tree_names().into_iter().filter_map(|id| {
            if id.len() != 42 {
                return None;
            }
            let mut bytes = [0; 42];
            bytes.copy_from_slice(&id[..]);
            Some(StreamId::from_bytes(bytes))
        })
    }

    pub fn contains(&self, id: StreamId) -> bool {
        chunk_file(&self.chunks, id).exists()
    }

    pub fn get(&self, id: StreamId) -> Result<Stream> {
        let nodes = open_nodes(&self.db, id)?;
        let path = chunk_file(&self.chunks, id);
        let tree = if !path.exists() {
            let tree = Tree::new(nodes, *id.hash(), id.length())?;
            std::fs::create_dir(path.parent().unwrap())?;
            File::create(&path)?;
            tree
        } else {
            nodes.get(id.hash())?.unwrap()
        };
        Ok(Stream { id, tree, path })
    }

    pub fn insert(&self, path: impl AsRef<Path>) -> Result<Stream> {
        let path = path.as_ref();
        let id = StreamId::from_path(path)?;

        let mut input = BufReader::new(File::open(path)?);
        let nodes = open_nodes(&self.db, id)?;
        let mut hasher = TreeHasher::new(nodes);
        std::io::copy(&mut input, &mut hasher)?;
        let tree = hasher.finalize()?;

        let output = chunk_file(&self.chunks, id);
        std::fs::create_dir(output.parent().unwrap())?;
        std::fs::copy(path, &output)?;

        Ok(Stream {
            id,
            tree,
            path: output,
        })
    }

    pub fn remove(&self, id: StreamId) -> Result<()> {
        self.db.drop_tree(id.to_bytes())?;
        std::fs::remove_file(&chunk_file(&self.chunks, id))?;
        Ok(())
    }
}

fn chunk_file(root: &Path, id: StreamId) -> PathBuf {
    let hash = blake3::hash(&id.to_bytes()[..]);
    let mut h = [0; 64];
    hex::encode_to_slice(hash.as_bytes(), &mut h).unwrap();
    let hex = std::str::from_utf8(&h[..]).unwrap();
    root.join(&hex[..2]).join(hex)
}

fn open_nodes(db: &sled::Db, id: StreamId) -> Result<NodeStorage> {
    let tree = db.open_tree(id.to_bytes())?;
    Ok(NodeStorage::new(tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store() -> Result<()> {
        let data = [0x42; 2049];
        let range = Range::new(1024, 1024);
        std::fs::write("/tmp/f", &data[..])?;

        let store1 = StreamStorage::new("/tmp/store1")?;
        let stream1 = store1.insert("/tmp/f")?;
        let id = stream1.id();
        let slice = stream1.encode_range(&range)?;

        let store2 = StreamStorage::new("/tmp/store2")?;
        let stream2 = store2.get(id)?;
        stream2.decode_range(&range, &slice)?;

        let mut buf = Vec::with_capacity(range.length() as _);
        stream2.read_range(range)?.read_to_end(&mut buf)?;
        assert_eq!(buf, [0x42; 1024]);

        store1.remove(id)?;
        store2.remove(id)?;

        Ok(())
    }
}
