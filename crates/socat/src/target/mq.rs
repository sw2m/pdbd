use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mq {
    Read,
    Receive,
    Send,
    Write,
    Bidirectional,
    Plain,
}

impl Mq {
    pub(crate) fn word(word: &str) -> Option<Self> {
        match word {
            "POSIXMQ-READ" => Some(Self::Read),
            "POSIXMQ-RECEIVE" | "POSIXMQ-RECV" => Some(Self::Receive),
            "POSIXMQ-SEND" => Some(Self::Send),
            "POSIXMQ-WRITE" => Some(Self::Write),
            "POSIXMQ-BIDIRECTIONAL" => Some(Self::Bidirectional),
            "POSIXMQ" => Some(Self::Plain),
            _ => None,
        }
    }
}

impl fmt::Display for Mq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "POSIXMQ-READ",
            Self::Receive => "POSIXMQ-RECEIVE",
            Self::Send => "POSIXMQ-SEND",
            Self::Write => "POSIXMQ-WRITE",
            Self::Bidirectional => "POSIXMQ-BIDIRECTIONAL",
            Self::Plain => "POSIXMQ",
        })
    }
}
