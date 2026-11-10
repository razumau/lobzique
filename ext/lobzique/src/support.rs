//! Calls into Ruby that magnus has no direct helper for. The only module
//! allowed `unsafe` and `rb_sys::protect` (see `lib.rs`).
#![allow(unsafe_code)]
#![allow(clippy::disallowed_methods)]

use lobzique_core::html5::{DocumentParser, FragmentParser};
use magnus::{
    prelude::*,
    rb_sys::{protect, AsRawId, AsRawValue, FromRawValue},
    Error, RArray, RClass, RString, Ruby, Value,
};
use std::any::Any;
use std::ffi::{c_void, CString};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};

/// Work that can run without the GVL: a lobzique-core type, which can't
/// hold a Ruby value because the core doesn't depend on Ruby, done a step
/// at a time. Sealed, so the list of such types is the one below.
pub trait Resumable: sealed::Sealed {
    /// Does the next piece of the work; false once there is none left.
    fn step(&mut self) -> bool;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::DocumentParser {}
    impl Sealed for super::FragmentParser {}
}

impl Resumable for DocumentParser {
    fn step(&mut self) -> bool {
        DocumentParser::step(self)
    }
}

impl Resumable for FragmentParser {
    fn step(&mut self) -> bool {
        FragmentParser::step(self)
    }
}

/// Steps `work` to the end without the GVL (`rb_thread_call_without_gvl`),
/// so other threads run meanwhile. An interrupt (`Thread#raise`, `Timeout`,
/// a signal) stops it after the current step; Ruby then handles the
/// interrupt with the GVL, and the work goes on from where it stopped
/// unless that raised. A panic in `work` resumes here, with the GVL.
pub fn without_gvl<W: Resumable>(work: &mut W) -> Result<(), Error> {
    let interrupted = AtomicBool::new(false);
    let mut run = Run {
        work,
        interrupted: &interrupted,
        done: false,
        panic: None,
    };
    while !run.done {
        interrupted.store(false, Ordering::Relaxed);
        let data = (&raw mut run).cast::<c_void>();
        let flag = (&raw const interrupted).cast_mut().cast::<c_void>();
        // SAFETY: `run` and `interrupted` outlive the call, which returns
        // only once `step_until_interrupted` has. That function catches
        // panics, and `set_interrupted` can't panic. `protect` catches what
        // Ruby raises when it handles an interrupt before or after them;
        // the closure owns nothing that a jump out of it would leak.
        let result = protect(|| unsafe {
            rb_sys::rb_thread_call_without_gvl(
                Some(step_until_interrupted::<W>),
                data,
                Some(set_interrupted),
                flag,
            );
            rb_sys::Qnil as rb_sys::VALUE
        });
        if let Some(panic) = run.panic.take() {
            std::panic::resume_unwind(panic);
        }
        result?;
    }
    Ok(())
}

/// `rb_thread_check_ints`: lets Ruby handle interrupts, and switch threads
/// if this one's time slice is over, as it does between Ruby calls.
pub fn check_interrupts() -> Result<(), Error> {
    // SAFETY: `protect` turns what an interrupt raises into an `Err`.
    protect(|| unsafe {
        rb_sys::rb_thread_check_ints();
        rb_sys::Qnil as rb_sys::VALUE
    })?;
    Ok(())
}

/// What the function run without the GVL reaches through its one pointer.
/// `interrupted` is also written from the thread that interrupts.
struct Run<'a, W> {
    work: &'a mut W,
    interrupted: &'a AtomicBool,
    done: bool,
    panic: Option<Box<dyn Any + Send>>,
}

/// One step at least, so interrupts that don't raise can't stall the work.
unsafe extern "C" fn step_until_interrupted<W: Resumable>(data: *mut c_void) -> *mut c_void {
    // SAFETY: `without_gvl` passes its `Run`, which nothing else touches
    // during the call.
    let run = unsafe { &mut *data.cast::<Run<'_, W>>() };
    let stepped = std::panic::catch_unwind(AssertUnwindSafe(|| loop {
        if !run.work.step() {
            return true;
        }
        if run.interrupted.load(Ordering::Relaxed) {
            return false;
        }
    }));
    match stepped {
        Ok(done) => run.done = done,
        Err(panic) => {
            run.done = true;
            run.panic = Some(panic);
        }
    }
    std::ptr::null_mut()
}

/// The unblocking function, which Ruby calls from the interrupting thread.
unsafe extern "C" fn set_interrupted(flag: *mut c_void) {
    // SAFETY: `without_gvl` passes its `AtomicBool`, which outlives the call.
    unsafe { &*flag.cast::<AtomicBool>() }.store(true, Ordering::Relaxed);
}

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

/// Ruby's `rb_apply`: calls `method` on `receiver`, private or not, as
/// `rb_funcall2` does, with the elements of `arguments`. An Array keeps the
/// arguments where the GC sees them; magnus has no call that takes one.
pub fn apply(
    ruby: &Ruby,
    receiver: Value,
    method: &str,
    arguments: RArray,
) -> Result<Value, Error> {
    let id = ruby.intern(method).as_raw();
    // SAFETY: `receiver` and `arguments` are live values on the stack, and
    // `protect` turns an exception raised by the method into an `Err`.
    let result =
        protect(|| unsafe { rb_sys::rb_apply(receiver.as_raw(), id, arguments.as_raw()) })?;
    // SAFETY: `rb_apply` returns a live value.
    Ok(unsafe { Value::from_raw(result) })
}

/// Yields the new object to the constructor's block, if it was given one.
pub fn yield_to_block(ruby: &Ruby, object: Value) -> Result<(), Error> {
    if ruby.block_given() {
        ruby.yield_value::<_, Value>(object)?;
    }
    Ok(())
}

/// Nokogiri's `NOKO_WARN_DEPRECATION`: printed only when `$VERBOSE` is true
/// and `Warning[:deprecated]` is on, prefixed with the caller's location.
pub fn warn_deprecated(message: &str) -> Result<(), Error> {
    // %s stops at a zero byte, as it does for Nokogiri's C strings
    let message = message.split('\0').next().unwrap_or_default();
    let message = CString::new(message).expect("no zero bytes left");
    // SAFETY: the format takes one C string, and `protect` turns what a
    // `Warning.warn` override raises into an `Err`.
    protect(|| unsafe {
        rb_sys::rb_category_warning(
            rb_sys::rb_warning_category_t::RB_WARN_CATEGORY_DEPRECATED,
            c"%s".as_ptr(),
            message.as_ptr(),
        );
        rb_sys::Qnil as rb_sys::VALUE
    })?;
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
    // SAFETY: the converted string is a live String on the stack.
    let string = RString::from_value(unsafe { Value::from_raw(string) })
        .expect("rb_string_value_cstr returns a String");
    Ok(string_bytes(string))
}

/// A copy of the string's bytes.
pub fn string_bytes(string: RString) -> Vec<u8> {
    // SAFETY: the bytes are copied before any Ruby code could change them.
    unsafe { string.as_slice() }.to_vec()
}
