//! Calls into Ruby that magnus has no direct helper for.

use magnus::{
    prelude::*,
    rb_sys::{protect, AsRawValue},
    Error, KwArgs, Ruby, Value,
};

/// Ruby's `rb_obj_call_init`: runs `initialize` with the arguments and block
/// the native constructor was called with, as Nokogiri's constructors do.
pub fn call_initialize(object: Value, args: &[Value]) -> Result<(), Error> {
    let argc = i32::try_from(args.len()).expect("argument count fits in an int");
    let argv = args.as_ptr().cast::<rb_sys::VALUE>();
    // SAFETY: `object` and `args` are live Ruby values on the stack, and
    // `protect` turns an exception raised by `initialize` into an `Err`.
    protect(|| unsafe {
        rb_sys::rb_obj_call_init_kw(object.as_raw(), argc, argv, 0);
        rb_sys::Qnil as rb_sys::VALUE
    })?;
    Ok(())
}

/// Yields the new object to the constructor's block, if it was given one.
pub fn yield_to_block(ruby: &Ruby, object: Value) -> Result<(), Error> {
    if ruby.block_given() {
        ruby.yield_value::<_, Value>(object)?;
    }
    Ok(())
}

/// `rb_category_warning(RB_WARN_CATEGORY_DEPRECATED, ...)`: printed only
/// when `Warning[:deprecated]` is on.
pub fn warn_deprecated(ruby: &Ruby, message: &str) -> Result<(), Error> {
    let options = ruby.hash_new();
    options.aset(ruby.to_symbol("category"), ruby.to_symbol("deprecated"))?;
    ruby.module_kernel()
        .funcall::<_, _, Value>("warn", (message, KwArgs(options)))?;
    Ok(())
}

/// Optional positional argument `index`, `nil` when absent.
pub fn optional_arg(ruby: &Ruby, args: &[Value], index: usize) -> Value {
    args.get(index)
        .copied()
        .unwrap_or_else(|| ruby.qnil().as_value())
}
