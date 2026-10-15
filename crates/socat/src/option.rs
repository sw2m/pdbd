use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unchecked(Vec<Opt>);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    mux: bool,
    cap: Option<NonZeroU32>,
    rest: Unchecked,
}

impl Opt {
    pub fn new(name: &str, value: Option<&str>) -> Option<Self> {
        let _ = (name, value);
        todo!()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

impl Unchecked {
    pub fn new(opts: Vec<Opt>) -> Self {
        Self(opts)
    }

    pub fn as_slice(&self) -> &[Opt] {
        &self.0
    }
}

impl Options {
    pub fn new(
        mux: bool,
        cap: Option<NonZeroU32>,
        rest: Unchecked,
    ) -> Option<Self> {
        let _ = (mux, cap, rest);
        todo!()
    }

    pub fn mux(&self) -> bool {
        self.mux
    }

    pub fn cap(&self) -> Option<NonZeroU32> {
        self.cap
    }

    pub fn rest(&self) -> &Unchecked {
        &self.rest
    }
}
