use crate::{Mime, Range, Result, StreamId, Tree, TreeHasher, TreeStorage};
use anyhow::Context;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub struct StreamHasher {
    path: PathBuf,
    chunks: BufWriter<File>,
    hasher: TreeHasher,
}

impl StreamHasher {
    fn new(storage: TreeStorage, path: PathBuf) -> Result<Self> {
        let chunks = BufWriter::new(File::create(&path)?);
        let hasher = TreeHasher::new(storage);
        Ok(Self {
            path,
            chunks,
            hasher,
        })
    }

    pub fn finalize(mut self, mime: Mime) -> Result<StreamId> {
        self.flush()?;
        let tree = self.hasher.finalize()?;
        let id = StreamId::new(*tree.hash(), tree.length()?.unwrap(), mime as _);
        let bytes64 = id.to_base64();
        let name = std::str::from_utf8(&bytes64).unwrap();
        let final_path = self.path.parent().unwrap().join(name);
        std::fs::rename(self.path, final_path)?;
        Ok(id)
    }
}

impl Write for StreamHasher {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.chunks.write(buf)?;
        self.hasher.write_all(&buf[..n])?;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.chunks.flush()?;
        self.hasher.flush()?;
        Ok(())
    }
}

pub struct StreamSlicer {
    chunks: BufReader<File>,
    tree: Tree,
}

impl StreamSlicer {
    fn new(path: &Path, tree: Tree) -> Result<Self> {
        let chunks = BufReader::new(File::open(path)?);
        Ok(Self { chunks, tree })
    }

    pub fn read_range_to(&mut self, range: &Range, to: &mut impl Write) -> Result<()> {
        self.tree.encode_range_to(range, to, &mut self.chunks)
    }

    pub fn read_range(&mut self, range: &Range) -> Vec<u8> {
        self.tree.encode_range(range, &mut self.chunks)
    }
}

pub struct StreamWriter {
    chunks: BufWriter<File>,
    tree: Tree,
}

impl StreamWriter {
    fn new(path: &Path, tree: Tree) -> Result<Self> {
        let chunks = BufWriter::new(File::open(path)?);
        Ok(Self { chunks, tree })
    }

    pub fn write_range_from(&mut self, range: &Range, from: &mut impl Read) -> Result<()> {
        self.tree.decode_range_from(range, from, &mut self.chunks)?;
        self.chunks.flush()?;
        Ok(())
    }

    pub fn write_range(&mut self, range: &Range, slice: &[u8]) -> Result<()> {
        self.tree.decode_range(range, slice, &mut self.chunks)?;
        self.chunks.flush()?;
        Ok(())
    }
}

pub struct StreamReader {
    chunks: BufReader<File>,
    tree: Tree,
    range: Range,
}

impl StreamReader {
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

impl Read for StreamReader {
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

impl Seek for StreamReader {
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

pub struct StreamStorage {
    chunks: PathBuf,
    storage: TreeStorage,
}

impl StreamStorage {
    pub fn new(path: PathBuf) -> Result<Self> {
        let chunks = path.join("chunks");
        std::fs::create_dir_all(&chunks)?;
        let db = sled::open(path.join("trees"))?;
        let tree = db.open_tree("trees")?;
        let storage = TreeStorage::new(tree);
        Ok(Self { chunks, storage })
    }

    fn stream_path(&self, id: StreamId) -> PathBuf {
        let bytes64 = id.to_base64();
        let name = std::str::from_utf8(&bytes64).unwrap();
        let mut path = PathBuf::with_capacity(self.chunks.as_os_str().len() + bytes64.len() + 1);
        path.push(&self.chunks);
        path.push(name);
        path
    }

    fn temp_path(&self) -> PathBuf {
        let mut b = [0; 8];
        getrandom::getrandom(&mut b).unwrap();
        let name = format!(
            "tmp_{:x}{:x}{:x}{:x}{:x}{:x}{:x}{:x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]
        );
        self.chunks.join(name)
    }

    pub fn streams(&self) -> impl Iterator<Item = Result<StreamId>> {
        // TODO:
        std::iter::empty()
    }

    pub fn contains(&self, id: StreamId) -> bool {
        // TODO: use db
        self.stream_path(id).exists()
    }

    pub fn remove(&self, id: StreamId) -> Result<()> {
        // TODO: remove tree
        std::fs::remove_file(self.stream_path(id))?;
        Ok(())
    }

    pub fn hasher(&self) -> Result<StreamHasher> {
        // TODO: handle failure
        StreamHasher::new(self.storage.clone(), self.temp_path())
    }

    pub fn add(&self, path: &Path) -> Result<StreamId> {
        let mime = Mime::from_path(path).unwrap_or_default();
        let mut input = BufReader::new(File::open(path)?);
        let mut hasher = self.hasher()?;
        std::io::copy(&mut input, &mut hasher)?;
        hasher.finalize(mime)
    }

    pub fn reader(&self, id: StreamId, range: Range) -> Result<StreamReader> {
        let tree = self.storage.get(id.hash())?.context("missing stream")?;
        StreamReader::new(&self.stream_path(id), tree, range)
    }

    pub fn writer(&self, id: StreamId) -> Result<StreamWriter> {
        let tree = self.storage.get(id.hash())?.context("missing stream")?;
        StreamWriter::new(&self.stream_path(id), tree)
    }

    pub fn slicer(&self, id: StreamId) -> Result<StreamSlicer> {
        let tree = self.storage.get(id.hash())?.context("missing stream")?;
        StreamSlicer::new(&self.stream_path(id), tree)
    }
}
