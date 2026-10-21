use std::ops::Range;

// #83
#[derive(Debug, Default)]
pub struct Lane {
    buf: Vec<u8>,
    head: usize,
    tail: usize,
    wrap: usize,
}

impl Lane {
    pub fn reserve(&mut self, n: usize) -> &mut [u8] {
        let at = if self.wrap == 0 && self.tail + n <= self.buf.len() {
            self.tail += n;
            self.tail - n
        } else if self.wrap + n <= self.head {
            self.wrap += n;
            self.wrap - n
        } else {
            self.grow(n);
            self.tail += n;
            self.tail - n
        };
        &mut self.buf[at..at + n]
    }

    fn grow(&mut self, n: usize) {
        let used = self.len();
        let size = (self.buf.len() * 2).max(used + n);
        let mut buf = Vec::with_capacity(size);
        buf.extend_from_slice(&self.buf[self.head..self.tail]);
        buf.extend_from_slice(&self.buf[..self.wrap]);
        buf.resize(size, 0);
        self.buf = buf;
        (self.head, self.tail, self.wrap) = (0, used, 0);
    }

    pub fn front(&self) -> &[u8] {
        &self.buf[self.head..self.tail]
    }

    pub fn take(&mut self, n: usize) -> Range<usize> {
        let range = self.head..self.head + n;
        self.head += n;
        if self.head == self.tail {
            (self.head, self.tail, self.wrap) = (0, self.wrap, 0);
        }
        range
    }

    pub fn slice(&self, range: Range<usize>) -> &[u8] {
        &self.buf[range]
    }

    pub fn len(&self) -> usize {
        self.tail - self.head + self.wrap
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }
}

#[cfg(test)]
mod test;
