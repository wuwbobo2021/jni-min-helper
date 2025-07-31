//! Minimal helper for `jni-rs`, supporting dynamic proxies, Android dex embedding,
//! permission request and broadcast receiver. Used for calling Java code from Rust.
//!
//! Please consider using [java-spaghetti](https://github.com/Dirbaio/java-spaghetti)
//! when its `0.3.0` version becomes available.
//!
//! `jni` is re-exported here for the user to import `jni` functions, avoiding
//! version inconsistency between `jni` and this crate.
//!
//! This crate uses [ndk_context::AndroidContext] on Android, usually initialized
//! by `android_activity`. Examples for Android are provided in the crate page.
//!
//! Please make sure you are viewing documentation generated for your target.

pub use jni;

pub use {convert::*, loader::*};

#[cfg(feature = "proxy")]
pub use proxy::*;

#[cfg(feature = "proxy")]
#[cfg(target_os = "android")]
pub use {permission::*, receiver::*};

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

#[cfg(feature = "proxy")]
mod proxy;

#[cfg(feature = "proxy")]
#[cfg(target_os = "android")]
mod receiver;

#[cfg(feature = "proxy")]
#[cfg(target_os = "android")]
mod permission;

use jni::{
    errors::Error,
    objects::{GlobalRef, JObject},
    JNIEnv, JavaVM,
};
use std::{cell::Cell, sync::OnceLock};

type AutoLocal<'a> = jni::objects::AutoLocal<'a, JObject<'a>>;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

thread_local! {
    static LAST_CLEARED_EX: Cell<Option<GlobalRef>> = const { Cell::new(None) };
}

/// Workaround for <https://github.com/jni-rs/jni-rs/issues/558>.
/// Calls `jni_get_vm()`, attaches the current thread to the JVM and executes the closure;
/// then clears the exception if the closure's result is `Err(Error::JavaException)`.
///
/// However, *use* `jni-min-helper` methods or `.map_err(jni_clear_ex)` before returning from the closure
/// if there may be temporary structs created in the closure with `drop()` doing JNI operations not listed in
/// <https://docs.oracle.com/javase/8/docs/technotes/guides/jni/spec/design.html#exception_handling>.
///
/// The thread may be dettached if it has not been attached previously.
#[inline(always)]
pub fn jni_with_env<R>(f: impl FnOnce(&mut JNIEnv) -> Result<R, Error>) -> Result<R, Error> {
    let vm = unsafe { jni_get_vm() };
    let mut guarded_env = vm.attach_current_thread()?;
    f(&mut guarded_env).map_err(jni_clear_ex)
}

/// Calls `jni_get_vm()` and tries attaching the current thread to the JVM permanently,
/// in order to make `jni_with_env` faster. Does nothing and returns false if the thread
/// is currently attached (this behaviour is determined by `jni-rs`).
///
/// Note: This blocks JVM exit; `AttachCurrentThreadAsDaemon` is not used as it is unsafe.
///
/// To avoid the fatal error "Native thread exiting without having called DetachCurrentThread", check
/// <https://doc.rust-lang.org/stable/std/thread/struct.LocalKey.html#platform-specific-behavior>.
pub fn jni_attach_permanently() -> bool {
    let vm = unsafe { jni_get_vm() };
    if vm.get_env().is_ok() {
        return false;
    }
    vm.attach_current_thread_permanently().is_ok()
}

/// Tells this crate to use an existing JVM, instead of launching a new JVM
/// with no arguments (which may panic on failure). Not available on Android.
///
/// Does nothing and returns false if it has been set previously.
///
/// # Safety
///
/// Do not terminate the corresponding JVM within the application's lifetime.
#[cfg(not(target_os = "android"))]
pub unsafe fn jni_set_vm(vm: &JavaVM) -> bool {
    if JAVA_VM.get().is_some() {
        false
    } else {
        let vm = unsafe { JavaVM::from_raw(vm.get_java_vm_pointer()).unwrap() };
        JAVA_VM.set(vm).unwrap();
        true
    }
}

/// Gets the remembered `JavaVM`, otherwise it launches a new JVM with no arguments
/// (which may panic on failure).
///
/// # Safety
///
/// `JavaVM` must not have `'static` lifetime, otherwise it will be possible to get
/// `JNIEnv<'static>` and have local references that outlive the `AttachGuard`:
/// <https://github.com/jni-rs/jni-rs/issues/558>.
///
/// Never call `JavaVM::destroy(&self)`.
#[cfg(not(target_os = "android"))]
#[inline(always)]
pub unsafe fn jni_get_vm() -> JavaVM {
    let raw_vm = JAVA_VM
        .get_or_init(|| {
            let args = jni::InitArgsBuilder::new().build().unwrap();
            JavaVM::new(args).unwrap()
        })
        .get_java_vm_pointer();
    jni::JavaVM::from_raw(raw_vm.cast()).unwrap()
}

/// Gets the `JavaVM` from current Android context.
///
/// # Safety
///
/// `JavaVM` must not have `'static` lifetime, otherwise it will be possible to get
/// `JNIEnv<'static>` and have local references that outlive the `AttachGuard`:
/// <https://github.com/jni-rs/jni-rs/issues/558>.
///
/// Never call `JavaVM::destroy(&self)`.
#[cfg(target_os = "android")]
#[inline(always)]
pub unsafe fn jni_get_vm() -> JavaVM {
    let raw_vm = JAVA_VM
        .get_or_init(|| {
            let ctx = ndk_context::android_context();
            // Safety: as documented in `ndk-context` to obtain the `jni::JavaVM`
            unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap()
        })
        .get_java_vm_pointer();
    jni::JavaVM::from_raw(raw_vm.cast()).unwrap()
}

/// It calls `JNIEnv::exception_clear()` which is needed for handling Java exceptions,
/// Not clearing it may cause the native program to crash on the next JNI call.
/// Heavily used inside this crate, with `Result::map_err()`.
///
/// Note: Dropping the `jni::AttachGuard` before clearing the exception may cause a
/// FATAL EXCEPTION that crashes the application, unless the thread has been attached
/// to the JVM permanently.
///
/// TODO: investigate the possibility of registering the `UncaughtExceptionHandler`,
/// and even posting a dead loop of a try-catch block for `Looper.loop()` to the Java
/// side main looper.
#[inline]
pub fn jni_clear_ex(err: Error) -> Error {
    jni_clear_ex_inner(err, true, true)
}

/// It is the same as `jni_clear_ex()` without printing error information. Use it with
/// `Result::map_err()` prior to functions from this crate to avoid error printing.
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

/// Takes away the stored reference of `java.lang.Throwable` of the last
/// Java exception cleared inside this crate (current thread).
#[inline(always)]
pub fn jni_last_cleared_ex() -> Option<GlobalRef> {
    LAST_CLEARED_EX.take()
}

#[inline(always)]
fn jni_clear_ex_inner(err: Error, print_err: bool, store_ex: bool) -> Error {
    let thread_id = std::thread::current().id();

    if let Error::JavaException = err {
        let _ = jni_with_env(|env| {
            if env.exception_check().unwrap_or(true) {
                if !print_err && !store_ex {
                    env.exception_clear().unwrap();
                    return Ok(());
                }

                let ex = env.exception_occurred(); // returns Result<JThrowable<'local>>

                #[cfg(not(target_os = "android"))]
                if print_err {
                    // This (and Java `printStackTrace()` with `PrintWriter`) may not work on Android.
                    // Note: Don't do this before `exception_check()` or `exception_occurred()`!
                    let _ = env.exception_describe();
                }

                // panics if unable to clear
                env.exception_clear().unwrap();

                if print_err {
                    #[cfg(target_os = "android")]
                    if let Ok(ex) = ex.as_ref() {
                        // This is required for Android because `env.exception_describe()` may not work.
                        if let Ok(ex_msg) = ex.get_throwable_msg(env) {
                            let ex_type = class_name_to_java(&ex.get_class_name(env).unwrap());
                            warn!("Exception in thread \"{thread_id:?}\" {ex_type}: {ex_msg}");
                        } else {
                            warn!("Unknown Java exception in thread \"{thread_id:?}\"");
                        }
                    }
                    // prints for all platforms
                    print_rust_stack();
                }

                if store_ex {
                    if let Ok(ex) = ex.global_ref(env) {
                        // prepare for `jni_last_cleared_ex()`
                        LAST_CLEARED_EX.set(Some(ex));
                    }
                } else {
                    let _ = ex.auto_local(env);
                }
            }
            Ok(())
        });
    } else if print_err {
        warn!("JNI Error in thread \"{thread_id:?}\": {err:?}");
        print_rust_stack();
    }
    err
}

fn print_rust_stack() {
    use std::backtrace::*;

    #[cfg(not(target_os = "android"))]
    {
        let backtrace = Backtrace::capture();
        if let BacktraceStatus::Captured = backtrace.status() {
            warn!("{}", backtrace);
        }
    }

    // `RUST_BACKTRACE` environment variable may not work on Android.
    #[cfg(target_os = "android")]
    warn!("\n{}", Backtrace::force_capture());
}

/// Used for calling `jni_clear_ex()` and turning an owned `JObject<'_>` reference (which leaks
/// memory on dropping in a Rust main thread permanently attached to the JVM) into an `AutoLocal`
/// which deletes the reference from the environment on dropping. Works with `android_activity`.
/// Note that borrowed references (`&JObject<'_>`) doesn't cause memory leak.
///
/// Performance penalty of using `AutoLocal<'_>` can be more serious than using local frames.
/// However, functions in this crate all return `AutoLocal`; to take advantage of a fixed-size
/// local reference frame while looping for a known amount of times, call `AutoLocal::forget()`.
/// Note that `JNIEnv::with_local_frame` is actually unsound, but it is safe if the inner closure
/// does not use any `AttachGuard` or `JNIEnv` obtained outside.
///
/// Reference:
/// - <https://github.com/jni-rs/jni-rs/issues/392#issuecomment-1343685851>
/// - <https://github.com/jni-rs/jni-rs/issues/548>
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
        let local = self.auto_local(env);
        local.globalize(env)
    }
}

// `impl<'a> JObjectAutoLocal<'a> for Result<AutoLocal<'a>, Error>`
// will cause a compilation error of conflicting implementation:
// upstream crates may add a new impl of trait `std::convert::From<AutoLocal<'a>>`
// for type `jni::objects::JObject<'_>` in future versions.

/// Converts an `AutoLocal<'_>` to an `GlobalRef`.
pub trait AutoLocalGlobalize<'a> {
    fn globalize(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error>;
}

impl<'a> AutoLocalGlobalize<'a> for Result<AutoLocal<'a>, Error> {
    #[inline(always)]
    fn globalize(self, env: &JNIEnv<'a>) -> Result<GlobalRef, Error> {
        let local = self?;
        let global = env.new_global_ref(&local)?;
        if !local.is_null() {
            global.null_check("new_global_ref")?;
        }
        Ok(global)
    }
}
