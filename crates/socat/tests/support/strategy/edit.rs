use super::*;

pub(crate) fn check(s: &str) -> Result<(), TestCaseError> {
    match Address::parse(s) {
        Ok(a) => {
            let p = a.to_string();
            prop_assert_eq!(Address::parse(&p), Ok(a), "{:?} → {:?}", s, p);
        }
        Err(e) => {
            prop_assert!(e.at <= s.len(), "{:?}: {}", s, e);
            prop_assert!(s.is_char_boundary(e.at), "{:?}: {}", s, e);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) enum Edit {
    Insert(usize, u8),
    Delete(usize),
    Substitute(usize, u8),
}

impl Edit {
    pub(crate) fn apply(&self, s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        match *self {
            Self::Insert(i, x) => b.insert(i % (b.len() + 1), x),
            Self::Delete(i) if !b.is_empty() => {
                let i = i % b.len();
                b.remove(i);
            }
            Self::Substitute(i, x) if !b.is_empty() => {
                let i = i % b.len();
                b[i] = x;
            }
            _ => {}
        }
        b
    }
}

pub(crate) fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Substitute(i, b)),
    ]
}

pub(crate) fn keywords() -> Vec<String> {
    let mut all: Vec<String> = [
        "FILE", "OPEN", "GOPEN", "EXEC", "STDIO", "PTY", "TCP", "UDP", "UNIX",
        "LOCAL", "WS", "WSS", "HTTPS",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let roots = [
        "TCP",
        "UDP",
        "WS",
        "WSS",
        "WEBTRANSPORT",
        "UNIX",
        "VSOCK",
        "ABSTRACT",
    ];
    for root in roots {
        all.extend(modal(root, Mode::Connect, &[]));
        all.extend(modal(root, Mode::Listen, &[]));
    }
    all
}
