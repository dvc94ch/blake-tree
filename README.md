# blake tree

```rust
fn main() -> Result<()> {
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

    let mut chunks = BufReader::new(File::open(path)?);
    chunks.seek(SeekFrom::Start(range.offset()))?;
    let mut chunk = [0; 1024];
    chunks.read_exact(&mut chunk)?;
    assert_eq!(chunk, [0x42; 1024]);

    Ok(())
}
```

# License
Apache-2.0 OR MIT
