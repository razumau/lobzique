pub mod selector;

use magnus::{Error, Ruby};

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    selector::init(ruby)
}
