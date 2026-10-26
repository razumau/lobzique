//! Calls into Ruby that magnus has no direct helper for.

use magnus::{
    prelude::*,
    rb_sys::{protect, AsRawValue, FromRawValue},
    Error, KwArgs, RClass, RString, Ruby, Value,
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

/// `rb_obj_class`: the class without singleton classes, which every
/// decorated node has.
pub fn obj_class(value: Value) -> RClass {
    // SAFETY: `rb_obj_class` takes any object and returns a class.
    let class = unsafe { Value::from_raw(rb_sys::rb_obj_class(value.as_raw())) };
    RClass::from_value(class).expect("rb_obj_class returns a class")
}

/// How Ruby's own type errors (`Check_Type`, `TypedData_Get_Struct`) name
/// the class of a wrong argument.
pub fn displayed_class(value: Value) -> String {
    let raw = value.as_raw();
    if raw == rb_sys::Qnil as rb_sys::VALUE {
        "nil".into()
    } else if raw == rb_sys::Qtrue as rb_sys::VALUE {
        "true".into()
    } else if raw == rb_sys::Qfalse as rb_sys::VALUE {
        "false".into()
    } else {
        obj_class(value).inspect()
    }
}

/// `Check_Type(value, T_STRING)`: no `to_str` conversion.
pub fn check_string(ruby: &Ruby, value: Value) -> Result<(), Error> {
    if RString::from_value(value).is_some() {
        return Ok(());
    }
    let message = format!(
        "wrong argument type {} (expected String)",
        displayed_class(value)
    );
    Err(Error::new(ruby.exception_type_error(), message))
}

/// A string as Nokogiri hands it to libxml2 through `StringValueCStr`:
/// converted with `to_str`, refused with `ArgumentError` if it contains a
/// NUL character, then read as a C string, up to the first zero byte. The
/// bytes are kept in whatever encoding they are in.
pub fn c_string(value: Value) -> Result<Vec<u8>, Error> {
    let mut bytes = c_string_with_length(value)?;
    if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
        bytes.truncate(end);
    }
    Ok(bytes)
}

/// Like [`c_string`], but read with the string's own length, as
/// `xmlNewCDataBlock` reads it. Zero bytes stay, as in UTF-16.
pub fn c_string_with_length(value: Value) -> Result<Vec<u8>, Error> {
    let mut raw = value.as_raw();
    // SAFETY: `raw` is a live value on the stack; `protect` turns the
    // TypeError or ArgumentError `rb_string_value_cstr` raises into an `Err`.
    // It replaces `raw` with the converted string.
    let string = protect(|| unsafe {
        rb_sys::rb_string_value_cstr(&mut raw);
        raw
    })?;
    // SAFETY: the converted string is a live String on the stack, and its
    // bytes are copied before any Ruby code could change it.
    let string = RString::from_value(unsafe { Value::from_raw(string) })
        .expect("rb_string_value_cstr returns a String");
    Ok(unsafe { string.as_slice() }.to_vec())
}
