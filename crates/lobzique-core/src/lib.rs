//! The Ruby-free part of lobzique. Code here must not depend on magnus or
//! rb-sys, so Miri, sanitizers and fuzzers can run it without a Ruby VM.

pub fn hello(subject: &str) -> String {
    format!("Hello {subject}, from Rust!")
}

#[cfg(test)]
mod tests {
    use super::hello;

    #[test]
    fn test_hello() {
        assert_eq!("Hello world, from Rust!", hello("world"));
    }
}
