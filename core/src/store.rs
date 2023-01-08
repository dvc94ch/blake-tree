use crate::{Mime, Range, Result, StreamId, Tree, TreeHasher};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

fn missing_chunk(pos: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        format!("missing chunk at position {}", pos),
    )
}

pub struct RangeReader {
    chunks: BufReader<File>,
    tree: Tree,
    range: Range,
    pos: u64,
}

impl RangeReader {
    fn new(path: &Path, tree: Tree, range: Range) -> Result<Self> {
        anyhow::ensure!(tree.has_range(&range)?);
        let mut chunks = BufReader::new(File::open(path)?);
        let pos = chunks.seek(SeekFrom::Start(range.offset()))?;
        Ok(Self {
            chunks,
            tree,
            range,
            pos,
        })
    }

    pub fn set_range(&mut self, range: Range) -> Result<()> {
        anyhow::ensure!(self.tree.has_range(&range)?);
        self.range = range;
        self.pos = self.chunks.seek(SeekFrom::Start(range.offset()))?;
        Ok(())
    }
}

impl Read for RangeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let rest = self.range.end() - self.pos;
        let n = u64::min(rest, buf.len() as _) as usize;
        if n == 0 {
            return Ok(0);
        }
        let n = self.chunks.read(&mut buf[..n])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let current_pos = self.chunks.seek(from)?;
        if current_pos < self.range.offset() || current_pos >= self.range.end() {
            self.chunks.seek(SeekFrom::Start(self.pos))?;
            return Err(missing_chunk(current_pos));
        }
        self.pos = current_pos;
        Ok(current_pos)
    }
}

#[derive(Clone, Debug)]
pub struct Stream {
    tree: Tree,
    path: PathBuf,
}

impl Stream {
    pub fn id(&self) -> &StreamId {
        self.tree.id()
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
        let mut chunks = BufWriter::new(File::create(&self.path)?);
        self.tree.decode_range_from(range, from, &mut chunks)?;
        chunks.flush()?;
        Ok(())
    }

    pub fn decode_range(&self, range: &Range, slice: &[u8]) -> Result<()> {
        let mut chunks = BufWriter::new(File::create(&self.path)?);
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

#[derive(Clone)]
pub struct StreamStorage {
    chunks: PathBuf,
    db: sled::Db,
}

impl StreamStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let chunks = path.as_ref().join("chunks");
        std::fs::create_dir_all(&chunks)?;
        let db = sled::open(path)?;
        // TODO: crash recovery
        Ok(Self { chunks, db })
    }

    pub fn streams(&self) -> impl Iterator<Item = StreamId> {
        self.db
            .tree_names()
            .into_iter()
            .filter_map(|id| StreamId::from_bytes(&id).ok())
    }

    pub fn contains(&self, id: &StreamId) -> bool {
        chunk_file(&self.chunks, id).exists()
    }

    pub fn get(&self, id: &StreamId) -> Result<Stream> {
        let tree = Tree::open(&self.db, *id)?;
        let path = chunk_file(&self.chunks, id);
        if !path.exists() {
            std::fs::create_dir(path.parent().unwrap())?;
            let f = File::create(&path)?;
            f.set_len(id.length())?;
        }
        Ok(Stream { tree, path })
    }

    pub fn insert_path(&self, path: impl AsRef<Path>) -> Result<Stream> {
        let path = path.as_ref();
        let mime = Mime::from_path(path).unwrap_or_default();
        let mut reader = BufReader::new(File::open(path)?);
        self.insert(mime, &mut reader)
    }

    pub fn insert(&self, mime: Mime, reader: &mut impl Read) -> Result<Stream> {
        let mut randomness = [0; 8];
        getrandom::getrandom(&mut randomness).unwrap();
        let mut file_name = [0; 16];
        hex::encode_to_slice(&randomness, &mut file_name).unwrap();
        let tmp = self
            .chunks
            .join(std::str::from_utf8(&file_name[..]).unwrap());

        let mut chunks = BufWriter::new(File::create(&tmp)?);
        let mut hasher = TreeHasher::new();
        let mut writers = TwoWriters(&mut chunks, &mut hasher);
        std::io::copy(reader, &mut writers)?;
        writers.flush()?;
        let tree = hasher.finalize(&self.db, mime)?;

        let path = chunk_file(&self.chunks, tree.id());
        std::fs::create_dir(path.parent().unwrap())?;
        std::fs::rename(tmp, &path)?;

        Ok(Stream { tree, path })
    }

    pub fn remove(&self, id: &StreamId) -> Result<()> {
        self.db.drop_tree(id.to_bytes())?;
        std::fs::remove_file(&chunk_file(&self.chunks, id))?;
        Ok(())
    }
}

fn chunk_file(root: &Path, id: &StreamId) -> PathBuf {
    let hash = blake3::hash(&id.to_bytes()[..]);
    let mut h = [0; 64];
    hex::encode_to_slice(hash.as_bytes(), &mut h).unwrap();
    let hex = std::str::from_utf8(&h[..]).unwrap();
    root.join(&hex[..2]).join(hex)
}

struct TwoWriters<W1, W2>(W1, W2);

impl<W1: Write, W2: Write> Write for TwoWriters<W1, W2> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.0.write(buf)?;
        self.1.write_all(&buf[..n])?;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.1.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store() -> Result<()> {
        let data = [0x42; 2049];
        let range = Range::new(1024, 1024);
        std::fs::write("/tmp/f", &data[..])?;

        std::fs::remove_dir_all("/tmp/store1").ok();
        let store1 = StreamStorage::new("/tmp/store1")?;
        let stream1 = store1.insert_path("/tmp/f")?;
        let id = stream1.id();
        let slice = stream1.encode_range(&range)?;
        store1.remove(id)?;
        std::fs::remove_dir_all("/tmp/store1")?;

        std::fs::remove_dir_all("/tmp/store2").ok();
        let store2 = StreamStorage::new("/tmp/store2")?;
        let stream2 = store2.get(id)?;
        stream2.decode_range(&range, &slice)?;

        let mut buf = [0; 1024];
        stream2.read_range(range)?.read_exact(&mut buf)?;
        assert_eq!(buf, [0x42; 1024]);

        store2.remove(id)?;
        std::fs::remove_dir_all("/tmp/store2")?;

        Ok(())
    }
}
