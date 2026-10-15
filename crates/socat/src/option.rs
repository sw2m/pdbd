use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub mux: bool,
    pub cap: Option<NonZeroU32>,
    pub rest: Vec<Opt>,
}
