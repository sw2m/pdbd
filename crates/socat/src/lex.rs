use std::fmt;

#[allow(dead_code)] // red skeleton only
pub(crate) struct Quoted<'a>(pub(crate) &'a str);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
