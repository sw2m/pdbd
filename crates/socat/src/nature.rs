use crate::Reason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nature {
    Peer,
    Queue,
    Echo,
    Inert,
    Interactive,
    Constant,
}

impl Nature {
    pub(crate) fn reason(self) -> Option<Reason> {
        match self {
            Self::Peer => None,
            Self::Queue => Some(Reason::Queue),
            Self::Echo => Some(Reason::Echo),
            Self::Inert => Some(Reason::Inert),
            Self::Interactive => Some(Reason::Interactive),
            Self::Constant => Some(Reason::Constant),
        }
    }
}
