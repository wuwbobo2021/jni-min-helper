//! Minimal helper for `jni-rs`, supporting dynamic proxies, Android dex
//! embedding and broadcast receiver. Used for calling Java code from Rust.
//!
//! `jni` is re-exported here for the user to import `jni` functions from
//! here, avoiding version inconsistency between `jni` and this crate.
//!
//! Please make sure you are viewing documentation generated for your target.
//! Examples for Android are provided in the crate page.

pub use jni;

pub use {convert::*, loader::*, proxy::*};

#[cfg(not(target_os = "android"))]
macro_rules! warn {
    ($($arg:tt)+) => (eprintln!($($arg)+))
}

#[cfg(target_os = "android")]
macro_rules! warn {
    ($($arg:tt)+) => (log::warn!($($arg)+))
}

mod convert;
mod loader;
mod proxy;

use jni::{
    errors::Error,
    objects::{GlobalRef, JObject},
    JNIEnv,
};
use std::cell::Cell;

type AutoLocal<'a> = jni::objects::AutoLocal<'a, JObject<'a>>;

thread_local! {
    static LAST_CLEARED_EX: Cell<Option<GlobalRef>> = const { Cell::new(None) };
}

/// It calls `JNIEnv::exception_clear()` which is needed for handling Java exceptions,
/// Not clearing it may cause the native program to crash on the next JNI call.
/// Heavily used inside this crate, with `Result::map_err()`.
#[inline]
pub fn jni_clear_ex(err: Error) -> Error {
    jni_clear_ex_inner(err, true, true)
}

/// It is the same as `jni_clear_ex()` without printing exception information. Use it with
/// `Result::map_err()` prior to functions from this crate to avoid exception printing.
#[inline]
pub fn jni_clear_ex_silent(err: Error) -> Error {
    jni_clear_ex_inner(err, false, true)
}

/// It is the same as `jni_clear_ex_silent()` without storing the exception for
/// `jni_last_cleared_ex()`.
#[inline]
pub fn jni_clear_ex_ignore(err: Error) -> Error {
    jni_clear_ex_inner(err, false, false)
}

/// Takes away the stored global reference of `java.lang.Throwable` of the last
/// Java exception cleared inside this crate.
#[inline(always)]
pub fn jni_last_cleared_ex() -> Option<GlobalRef> {
    LAST_CLEARED_EX.take()
}

#[inline(always)]
fn jni_clear_ex_inner(err: Error, print_ex: bool, store_ex: bool) -> Error {
    if let Error::JavaException = err {
        let env = &mut jni_attach_vm().unwrap();
        if env.exception_check().unwrap_or(true) {
            let ex = env.exception_occurred();

            #[cfg(not(target_os = "android"))]
            if print_ex {
                // This (and Java `printStackTrace()` with `PrintWriter`) may not work on Android.
                // Don't do it before `exception_check()` or `exception_occurred()`!
                let _ = env.exception_describe();
            }
            env.exception_clear().unwrap(); // panic if unable to clear

            if let Ok(ex) = ex.global_ref(env) {
                #[cfg(target_os = "android")]
                if print_ex {
                    if let Ok(ex_msg) = ex.get_throwable_msg(env) {
                        let thread_id = std::thread::current().id();
                        let ex_type = class_name_to_java(&ex.get_class_name(env).unwrap());
                        warn!("Exception in thread \"{thread_id:?}\" {ex_type}: {ex_msg}");
                    }
                }
                if store_ex {
                    // prepare for `jni_last_cleared_ex()`
                    LAST_CLEARED_EX.set(Some(ex));
                }
            }
        }
    }
    err
}

/// Used for calling `jni_clear_ex()` and turning an owned `JObject<'_>` reference (which leaks
/// memory on dropping in a Rust main thread permanently attached to the JVM) into an `AutoLocal`
/// which deletes the reference from the environment on dropping. Works with `android_activity`.
/// Note that borrowed references (`&JObject<'_>`) doesn't cause memory leak.
///
/// Performance penalty of using `AutoLocal<'_>` can be more serious than using local frames.
/// However, functions in this crate all return `AutoLocal`; to take advantage of a fixed-size
/// local reference frame while looping for a known amount of times, call `AutoLocal::forget()`.
/// Reference: <https://github.com/jni-rs/jni-rs/issues/392#issuecomment-1343685851>.
///
/// Turning a null reference into `AutoLocal<'_>` is acceptable, because the JNI `DeleteLocalRef`
/// doesn't require the reference to be non-null, while it's required for some other functions.
pub trait JObjectAutoLocal<'a> {
    fn auto_local(self, env: &JNIEnv<'a>) -> Result<AutoLocal<'a>, Error>;
    fn global_ref(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error>;
}

impl<'a, T> JObjectAutoLocal<'a> for Result<T, Error>
where
    T: Into<JObject<'a>>,
{
    #[inline(always)]
    fn auto_local(self, env: &JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        self.map(|o| env.auto_local(o.into())).map_err(jni_clear_ex)
    }

    #[inline(always)]
    fn global_ref(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error> {
        let local = self.auto_local(env)?;
        env.new_global_ref(local)
    }
}

/// Converts an `AutoLocal<'_>` to an `GlobalRef`.
pub trait AutoLocalGlobalize<'a> {
    fn globalize(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error>;
}

impl<'a> AutoLocalGlobalize<'a> for Result<AutoLocal<'a>, Error> {
    #[inline(always)]
    fn globalize(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error> {
        self.and_then(|o| env.new_global_ref(&o))
    }
}
