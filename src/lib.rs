use anyhow::Result;
use blake3::Hash;
use std::collections::{HashMap, VecDeque};
use std::io::{Cursor, Read};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Chunk {
    index: u64,
    length: u16,
    is_root: bool,
    hash: Hash,
}

impl Chunk {
    pub const SIZE: u64 = 1024;

    pub fn new(index: u64, bytes: &[u8], is_root: bool) -> Self {
        debug_assert!(bytes.len() as u64 <= Chunk::SIZE);
        let length = bytes.len() as u16;
        let hash = blake3::guts::ChunkState::new(index)
            .update(bytes)
            .finalize(is_root);
        Self {
            index,
            length,
            is_root,
            hash,
        }
    }

    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        anyhow::ensure!(bytes.len() as u64 <= Chunk::SIZE);
        let chunk = Self::new(self.index, bytes, self.is_root);
        anyhow::ensure!(chunk == *self);
        Ok(())
    }

    pub fn index_start(self) -> u64 {
        self.index
    }

    pub fn index_end(self) -> u64 {
        self.index + 1
    }

    pub fn offset_start(self) -> u64 {
        self.index * Chunk::SIZE
    }

    pub fn offset_end(self) -> u64 {
        self.offset_start() + self.length()
    }

    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub fn length(self) -> u64 {
        self.length as _
    }

    pub fn is_root(&self) -> bool {
        self.is_root
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubTree {
    parent: Hash,
    left: Hash,
    right: Hash,
    is_root: bool,
}

impl SubTree {
    pub fn new(left: Hash, right: Hash, is_root: bool) -> Self {
        let parent = blake3::guts::parent_cv(&left, &right, is_root);
        Self {
            parent,
            left,
            right,
            is_root,
        }
    }

    pub fn parent(&self) -> &Hash {
        &self.parent
    }

    pub fn left(&self) -> &Hash {
        &self.left
    }

    pub fn right(&self) -> &Hash {
        &self.right
    }

    pub fn is_root(&self) -> bool {
        self.is_root
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Node {
    Chunk(Chunk),
    Tree(SubTree),
}

impl Node {
    pub fn is_root(&self) -> bool {
        match self {
            Self::Chunk(chunk) => chunk.is_root(),
            Self::Tree(tree) => tree.is_root(),
        }
    }

    pub fn chunk(&self) -> Option<&Chunk> {
        if let Self::Chunk(chunk) = self {
            Some(chunk)
        } else {
            None
        }
    }

    pub fn tree(&self) -> Option<&SubTree> {
        if let Self::Tree(tree) = self {
            Some(tree)
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
pub struct Tree {
    nodes: HashMap<Hash, Node>,
    chunks: HashMap<u64, Hash>,
    trees: HashMap<Hash, Hash>,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, hash: &Hash) -> Option<&Node> {
        self.nodes.get(hash)
    }

    pub fn add_chunk(&mut self, index: u64, bytes: &[u8], is_root: bool) -> Hash {
        let chunk = Chunk::new(index, bytes, is_root);
        self.add_node(Node::Chunk(chunk))
    }

    pub fn add_tree(&mut self, left: Hash, right: Hash, is_root: bool) -> Hash {
        let tree = SubTree::new(left, right, is_root);
        self.add_node(Node::Tree(tree))
    }

    pub fn add_node(&mut self, node: Node) -> Hash {
        let hash = match &node {
            Node::Chunk(chunk) => {
                self.chunks.insert(chunk.index_start(), *chunk.hash());
                *chunk.hash()
            }
            Node::Tree(tree) => {
                self.trees.insert(*tree.left(), *tree.parent());
                self.trees.insert(*tree.right(), *tree.parent());
                *tree.parent()
            }
        };
        self.nodes.insert(hash, node);
        hash
    }

    pub fn depth(&self, hash: &Hash) -> Option<u64> {
        match self.get(hash)? {
            Node::Chunk(_) => Some(0),
            Node::Tree(tree) => Some(self.depth(tree.left())? + 1),
        }
    }

    pub fn index_start(&self, hash: &Hash) -> Option<u64> {
        match self.get(hash)? {
            Node::Chunk(chunk) => Some(chunk.index_start()),
            Node::Tree(tree) => self.index_start(tree.left()),
        }
    }

    pub fn index_end(&self, hash: &Hash) -> Option<u64> {
        match self.get(hash)? {
            Node::Chunk(chunk) => Some(chunk.index_end()),
            Node::Tree(tree) => self.index_end(tree.right()),
        }
    }

    pub fn index_length(&self, hash: &Hash) -> Option<u64> {
        Some(self.index_end(hash)? - self.index_start(hash)?)
    }

    pub fn offset_start(&self, hash: &Hash) -> Option<u64> {
        match self.get(hash)? {
            Node::Chunk(chunk) => Some(chunk.offset_start()),
            Node::Tree(tree) => self.offset_start(tree.left()),
        }
    }

    pub fn offset_end(&self, hash: &Hash) -> Option<u64> {
        match self.get(hash)? {
            Node::Chunk(chunk) => Some(chunk.offset_end()),
            Node::Tree(tree) => self.offset_end(tree.right()),
        }
    }

    pub fn offset_length(&self, hash: &Hash) -> Option<u64> {
        Some(self.offset_end(hash)? - self.offset_start(hash)?)
    }

    pub fn complete(&self, hash: &Hash) -> bool {
        match self.get(hash) {
            Some(Node::Chunk(_)) => true,
            Some(Node::Tree(tree)) => self.complete(tree.left()) && self.complete(tree.right()),
            None => false,
        }
    }

    pub fn extract(&self, i: u64, n: u64) -> Option<Tree> {
        let mut tree = Tree::new();
        let mut trees = vec![];
        for j in 0..n {
            let hash = self.chunks.get(&(i + j))?;
            let node = *self.get(hash)?;
            let chunk = node.chunk().unwrap();
            tree.add_node(node);
            if !node.is_root() {
                trees.push(*chunk.hash());
            }
        }
        while let Some(hash) = trees.pop() {
            let hash = self.trees.get(&hash)?;
            let node = *self.get(hash)?;
            let t = node.tree().unwrap();
            tree.add_node(node);
            if !node.is_root() {
                trees.push(*t.parent());
            }
        }
        Some(tree)
    }

    pub fn verify(&mut self, _root: &Hash, _other: &Self) -> Result<()> {
        todo!()
    }
}

#[derive(Debug)]
pub struct BlakeTree {
    tree: Tree,
    root: Hash,
}

impl BlakeTree {
    pub fn new(root: Hash) -> Self {
        Self {
            tree: Tree::new(),
            root,
        }
    }

    pub fn hash(input: &[u8]) -> Self {
        let mut tree = Tree::new();
        let length = input.len() as u64;
        let num_chunks = std::cmp::max((length + Chunk::SIZE - 1) / Chunk::SIZE, 1);
        let is_root = num_chunks == 1;
        let input = &mut Cursor::new(input);
        let mut buffer = Vec::with_capacity(Chunk::SIZE as _);
        let mut chunks = VecDeque::new();
        for i in 0..num_chunks {
            input.take(Chunk::SIZE).read_to_end(&mut buffer).unwrap();
            let hash = tree.add_chunk(i as _, &buffer, is_root);
            chunks.push_back(hash);
            buffer.clear();
        }
        let mut next_chunks = VecDeque::new();
        while chunks.len() > 1 {
            let is_root = chunks.len() == 2;
            loop {
                match chunks.len() {
                    0 => {
                        break;
                    }
                    1 => {
                        let hash = chunks.pop_front().unwrap();
                        next_chunks.push_back(hash);
                    }
                    _ => {
                        let left = chunks.pop_front().unwrap();
                        let right = chunks.pop_front().unwrap();
                        let hash = tree.add_tree(left, right, is_root);
                        next_chunks.push_back(hash);
                    }
                }
            }
            std::mem::swap(&mut chunks, &mut next_chunks);
        }
        let root = chunks.pop_front().unwrap();
        Self { root, tree }
    }

    pub fn extract(&self, i: u64, n: u64) -> Option<Self> {
        Some(Self {
            root: self.root,
            tree: self.tree.extract(i, n)?,
        })
    }

    pub fn verify(&mut self, other: &Self) -> Result<()> {
        anyhow::ensure!(self.root == other.root);
        self.tree.verify(&other.root, &other.tree)
    }

    pub fn root(&self) -> &Hash {
        &self.root
    }

    pub fn length(&self) -> Option<u64> {
        self.tree.offset_length(&self.root)
    }

    pub fn complete(&self) -> bool {
        self.tree.complete(&self.root)
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
        Chunk::SIZE - 1,
        Chunk::SIZE,
        Chunk::SIZE + 1,
        2 * Chunk::SIZE - 1,
        2 * Chunk::SIZE,
        2 * Chunk::SIZE + 1,
        3 * Chunk::SIZE - 1,
        3 * Chunk::SIZE,
        3 * Chunk::SIZE + 1,
        4 * Chunk::SIZE - 1,
        4 * Chunk::SIZE,
        4 * Chunk::SIZE + 1,
        8 * Chunk::SIZE - 1,
        8 * Chunk::SIZE,
        8 * Chunk::SIZE + 1,
        16 * Chunk::SIZE - 1,
        16 * Chunk::SIZE,
        16 * Chunk::SIZE + 1,
    ];

    #[test]
    fn test_hash_matches() {
        let buf = [0x42; 65537];
        for &case in TEST_CASES {
            dbg!(case);
            let input = &buf[..(case as _)];
            let expected = blake3::hash(input);
            let tree = BlakeTree::hash(input);
            dbg!(&tree);
            assert_eq!(tree.root, expected);
            assert!(tree.complete());
            assert_eq!(tree.length(), Some(case));
        }
    }
}
