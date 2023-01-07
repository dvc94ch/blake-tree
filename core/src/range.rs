use crate::CHUNK_SIZE;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Range {
    offset: u64,
    length: u64,
}

impl Range {
    pub fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
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
            let first = Range::new(self.offset, at);
            let second = Range::new(self.offset + at, self.length - at);
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

    pub fn extend(&mut self, amount: u64) {
        self.length += amount;
    }

    pub fn intersects(&self, other: &Range) -> bool {
        self == other ||
        // easier to write down when it does not intersect and invert
        // !(self.end() <= other.offset() || self.offset() >= other.end())
        // and use boolean algebra to simplify expression
        self.end() > other.offset() && self.offset() < other.end()
    }

    pub fn encoded_size(&self) -> u64 {
        const HEADER_SIZE: u64 = 8;
        const PARENT_SIZE: u64 = 32 * 2;
        let num_chunks = self.num_chunks();
        // num parents always one less than num chunks
        let num_parents = num_chunks - 1;
        HEADER_SIZE + PARENT_SIZE * num_parents + CHUNK_SIZE * num_chunks
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}..{}", self.offset, self.end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersects() {
        let ranges = [
            ((0, 0), (0, 0)),
            ((2, 5), (1, 6)),
            ((2, 5), (3, 4)),
            ((2, 4), (3, 5)),
            ((3, 5), (2, 4)),
        ];
        for ((a, b), (c, d)) in ranges {
            let a = Range::new(a, b);
            let b = Range::new(c, d);
            assert!(a.intersects(&b));
        }
    }

    #[test]
    fn test_doesnt_intersect() {
        let ranges = [((0, 0), (1, 0)), ((0, 1), (2, 1)), ((2, 5), (0, 1))];
        for ((a, b), (c, d)) in ranges {
            let a = Range::new(a, b);
            let b = Range::new(c, d);
            assert!(!a.intersects(&b));
        }
    }
}
