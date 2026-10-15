#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) at: usize,
    pub(crate) eq: Option<usize>,
}
