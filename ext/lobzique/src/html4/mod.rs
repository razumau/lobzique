pub mod document;

use magnus::{Error, Ruby};

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    document::init(ruby)
}
