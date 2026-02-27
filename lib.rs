//! Minimal helper for `jni-rs`, supporting dynamic proxies, Android dex embedding,
//! permission request and broadcast receiver. Used for calling Java code from Rust.
//!
//! `jni` is re-exported here for the user to import `jni` functions, avoiding
//! version inconsistency between `jni` and this crate.
//!
//! This crate uses [ndk_context::AndroidContext] on Android, usually initialized
//! by `android_activity`. Examples for Android are provided in the crate page.
//!
//! Please make sure you are viewing documentation generated for your target.

pub use jni;

pub use bindings::*;
pub use proxy::*;

#[cfg(target_os = "android")]
pub use {loader::*, permission::*, receiver::*};

#[cfg(not(target_os = "android"))]
macro_rules! warn {
    ($($arg:tt)+) => (eprintln!($($arg)+))
}

#[cfg(target_os = "android")]
macro_rules! warn {
    ($($arg:tt)+) => (log::warn!($($arg)+))
}

mod bindings;
mod loader;

mod proxy;

#[cfg(target_os = "android")]
mod receiver;

#[cfg(target_os = "android")]
mod permission;

use jni::{
    errors::Error,
    objects::{Global, JThrowable},
    Env, JavaVM,
};
use std::cell::Cell;

thread_local! {
    static LAST_CLEARED_EX: Cell<Option<Global<JThrowable>>> = const { Cell::new(None) };
}

/// Calls [jni_get_vm], attaches the current thread to the JVM and executes the closure;
/// The thread may stay attached even if it has not been attached previously.
#[inline(always)]
pub fn jni_with_env<R>(f: impl FnOnce(&mut Env) -> Result<R, Error>) -> Result<R, Error> {
    let vm = jni_get_vm();
    vm.attach_current_thread(f)
}

/// Try to get the `JavaVM` from  `jni::JavaVM::singleton`, otherwise it launches
/// a new JVM with no arguments (which may panic on failure).
#[cfg(not(target_os = "android"))]
#[inline(always)]
pub fn jni_get_vm() -> JavaVM {
    if let Ok(vm) = jni::JavaVM::singleton() {
        return vm;
    }
    let args = jni::InitArgsBuilder::new().build().unwrap();
    JavaVM::new(args).unwrap()
}

/// This is needed because the `JAVA_VM_SINGLETON` in `jni` crate somehow drops earlier than the
/// `OnceLock` defined in the current crate; the Java VM may not be destroyed between unit tests
/// because they may be executed in the same process.
#[allow(unused)]
#[cfg(not(target_os = "android"))]
pub(crate) fn jni_init_vm_for_unit_test() {
    use std::sync::OnceLock;
    static JAVA_VM: OnceLock<jni::JavaVM> = OnceLock::new();
    let raw_vm = JAVA_VM.get_or_init(|| {
        let args = jni::InitArgsBuilder::new().build().unwrap();
        jni::JavaVM::new(args).unwrap()
    });
}

/// Try to get the `JavaVM` from  `jni::JavaVM::singleton`, otherwise it gets
/// the `JavaVM` from current Android context.
#[cfg(target_os = "android")]
#[inline(always)]
pub fn jni_get_vm() -> JavaVM {
    let raw_vm = JAVA_VM
        .get_or_init(|| {
            let ctx = ndk_context::android_context();
            // Safety: as documented in `ndk-context` to obtain the `jni::JavaVM`
            unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap()
        })
        .get_java_vm_pointer();
    jni::JavaVM::from_raw(raw_vm.cast()).unwrap()
}
