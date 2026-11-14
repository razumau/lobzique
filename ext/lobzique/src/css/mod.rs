pub mod parser;
pub mod selector;

use magnus::{Error, Ruby};

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    selector::init(ruby)?;
    parser::init(ruby)
}
