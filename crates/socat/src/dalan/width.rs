#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    I32,
    U32,
    I64,
    U64,
    I16,
    U16,
    I8,
    U8,
}

impl Width {
    pub(crate) fn of(letter: char) -> Option<Self> {
        Some(match letter {
            'i' => Self::I32,
            'I' => Self::U32,
            'l' => Self::I64,
            'L' => Self::U64,
            's' => Self::I16,
            'S' => Self::U16,
            'b' => Self::I8,
            'B' => Self::U8,
            _ => return None,
        })
    }

    pub fn letter(self) -> char {
        match self {
            Self::I32 => 'i',
            Self::U32 => 'I',
            Self::I64 => 'l',
            Self::U64 => 'L',
            Self::I16 => 's',
            Self::U16 => 'S',
            Self::I8 => 'b',
            Self::U8 => 'B',
        }
    }

    pub fn fits(self, n: i128) -> bool {
        let (low, high) = match self {
            Self::I32 => (i32::MIN.into(), i32::MAX.into()),
            Self::U32 => (0, u32::MAX.into()),
            Self::I64 => (i64::MIN.into(), i64::MAX.into()),
            Self::U64 => (0, u64::MAX.into()),
            Self::I16 => (i16::MIN.into(), i16::MAX.into()),
            Self::U16 => (0, u16::MAX.into()),
            Self::I8 => (i8::MIN.into(), i8::MAX.into()),
            Self::U8 => (0, u8::MAX.into()),
        };
        (low..=high).contains(&n)
    }
}
