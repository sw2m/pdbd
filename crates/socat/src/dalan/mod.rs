// see socat(1) "dalan", the generic socket address
mod item;
mod width;

pub use item::Item;
pub use width::Width;

use crate::lex::Token;
use crate::{Error, Kind};
use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dalan(Vec<Item>);

impl Dalan {
    pub fn new(items: Vec<Item>) -> Option<Self> {
        let mut merged: Vec<Item> = Vec::with_capacity(items.len());
        for item in items {
            match (merged.last_mut(), item) {
                (Some(Item::Bytes(run)), Item::Bytes(more)) => run.extend(more),
                (_, item) => merged.push(item),
            }
        }
        let items = merged;
        let valid = !items.is_empty()
            && items.iter().all(|i| match i {
                Item::Bytes(b) => !b.is_empty(),
                Item::Int(w, n) => w.fits(*n),
                Item::Text(_) | Item::Char(_) => true,
            });
        valid.then_some(Self(items))
    }

    pub fn items(&self) -> &[Item] {
        &self.0
    }

    pub(crate) fn parse(tok: &Token) -> Result<Self, Error> {
        let text = tok.text.as_str();
        let bad = |at: usize| {
            let at = if tok.cooked { 0 } else { at };
            Error::new(tok.at + at, Kind::Dalan(text.into()))
        };
        let mut items: Vec<Item> = Vec::new();
        let mut chars = text.chars().peekable();
        let mut at = 0;
        let mut gap = false;
        while let Some(c) = chars.next() {
            let start = at;
            at += c.len_utf8();
            if c.is_ascii_whitespace() {
                gap = true;
                continue;
            }
            let item = match (c, items.last()) {
                ('x', _) => Self::bytes(None, &mut chars),
                ('"', _) => Self::text(&mut chars),
                ('\'', _) => Self::char(&mut chars),
                (c, _) if Width::of(c).is_some() => {
                    Width::of(c).and_then(|w| Self::int(w, None, &mut chars))
                }
                (c, Some(Item::Bytes(_))) if gap && c.is_ascii_hexdigit() => {
                    Self::bytes(Some(c), &mut chars)
                }
                (c, Some(Item::Int(w, _)))
                    if gap && (c.is_ascii_digit() || c == '-') =>
                {
                    Self::int(*w, Some(c), &mut chars)
                }
                _ => None,
            };
            let item = item.ok_or_else(|| bad(start))?;
            at = text.len() - chars.clone().map(char::len_utf8).sum::<usize>();
            gap = false;
            items.push(item);
        }
        Self::new(items).ok_or_else(|| bad(0))
    }

    fn bytes(first: Option<char>, chars: &mut Peekable<Chars>) -> Option<Item> {
        let mut hex: String = first.into_iter().collect();
        while let Some(&h) = chars.peek().filter(|h| h.is_ascii_hexdigit()) {
            hex.push(h);
            chars.next();
        }
        if hex.is_empty() || hex.len() % 2 != 0 {
            return None;
        }
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<Result<_, _>>()
            .ok()?;
        Some(Item::Bytes(bytes))
    }

    fn int(
        width: Width,
        first: Option<char>,
        chars: &mut Peekable<Chars>,
    ) -> Option<Item> {
        let mut digits: String = first.into_iter().collect();
        if first.is_none() && chars.peek() == Some(&'-') {
            digits.push('-');
            chars.next();
        }
        while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
            digits.push(d);
            chars.next();
        }
        let bare = digits.trim_start_matches('-');
        if bare.is_empty() || (bare.len() > 1 && bare.starts_with('0')) {
            return None;
        }
        let n: i128 = digits.parse().ok()?;
        width.fits(n).then_some(Item::Int(width, n))
    }

    fn text(chars: &mut Peekable<Chars>) -> Option<Item> {
        let mut text = String::new();
        loop {
            match chars.next()? {
                '"' => return Some(Item::Text(text)),
                '\\' => text.push(Item::unescape(chars.next()?)),
                c => text.push(c),
            }
        }
    }

    fn char(chars: &mut Peekable<Chars>) -> Option<Item> {
        let c = match chars.next()? {
            '\\' => Item::unescape(chars.next()?),
            '\'' => return None,
            c => c,
        };
        (chars.next()? == '\'').then_some(Item::Char(c))
    }
}

impl fmt::Display for Dalan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut hex = false;
        for item in &self.0 {
            let small = matches!(item, Item::Int(Width::I8 | Width::U8, _));
            if hex && small {
                f.write_str(" ")?;
            }
            write!(f, "{item}")?;
            hex = matches!(item, Item::Bytes(_));
        }
        Ok(())
    }
}
