use crate::frame::Reader;
use crate::Held;
use std::collections::VecDeque;
use std::ops::Range;

#[derive(Debug, Default)]
struct More {
    rest: VecDeque<Range<usize>>,
    spill: Vec<u8>,
    skip: usize,
}

#[derive(Debug, Default)]
pub struct Hold {
    head: Range<usize>,
    more: Option<Box<More>>,
}

impl Hold {
    pub const EMPTY: Self = Self {
        head: 0..0,
        more: None,
    };

    fn more(&mut self) -> &mut More {
        self.more.get_or_insert_default()
    }

    pub fn push(&mut self, piece: Range<usize>) {
        match self.head.is_empty() {
            true => self.head = piece,
            false => self.more().rest.push_back(piece),
        }
    }

    pub fn front(&self) -> Option<&Range<usize>> {
        (!self.head.is_empty()).then_some(&self.head)
    }

    pub fn consume(&mut self, mut n: usize) {
        if let Some(m) = self.more.as_mut() {
            let own = (m.spill.len() - m.skip).min(n);
            m.skip += own;
            n -= own;
            if m.skip == m.spill.len() {
                m.spill.clear();
                m.skip = 0;
            }
        }
        while n > 0 && !self.head.is_empty() {
            let k = self.head.len().min(n);
            self.head.start += k;
            n -= k;
            if self.head.is_empty() {
                let next = self.more.as_mut().and_then(|m| m.rest.pop_front());
                self.head = next.unwrap_or_default();
            }
        }
    }

    pub fn spill(&mut self, reader: &Reader) {
        if self.head.is_empty() {
            return;
        }
        let head = std::mem::take(&mut self.head);
        let m = self.more();
        m.spill.drain(..m.skip);
        m.skip = 0;
        m.spill.extend_from_slice(reader.slice(head));
        for piece in m.rest.drain(..) {
            m.spill.extend_from_slice(reader.slice(piece));
        }
    }

    pub fn clear(&mut self) {
        self.head = Range::default();
        if let Some(m) = self.more.as_mut() {
            m.rest.clear();
            m.spill.clear();
            m.skip = 0;
        }
    }

    pub fn lend<'a>(&'a self, reader: &'a Reader) -> Held<'a> {
        static NONE: VecDeque<Range<usize>> = VecDeque::new();
        let (first, rest) = match self.more.as_deref() {
            Some(m) => (&m.spill[m.skip..], m.rest.iter()),
            None => (&[][..], NONE.iter()),
        };
        Held::new(first, self.front(), rest, reader)
    }
}
