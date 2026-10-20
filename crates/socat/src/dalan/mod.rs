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
        let bad = || tok.fail(Kind::Dalan(tok.text.clone()));
        let mut items = Vec::new();
        let mut chars = tok.text.chars().peekable();
        while let Some(c) = chars.next() {
            let item = match c {
                c if c.is_whitespace() => continue,
                'x' => Self::bytes(&mut chars),
                '"' => Self::text(&mut chars),
                '\'' => Self::char(&mut chars),
                c => Width::of(c).and_then(|w| Self::int(w, &mut chars)),
            };
            items.push(item.ok_or_else(bad)?);
        }
        Self::new(items).ok_or_else(bad)
    }

    fn bytes(chars: &mut Peekable<Chars>) -> Option<Item> {
        let mut hex = String::new();
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

    fn int(width: Width, chars: &mut Peekable<Chars>) -> Option<Item> {
        let mut digits = String::new();
        if chars.peek() == Some(&'-') {
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
