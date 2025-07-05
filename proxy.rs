use crate::{
    convert::*, jni_clear_ex, jni_last_cleared_ex, jni_with_env, loader::*, AutoLocal,
    AutoLocalGlobalize, JObjectAutoLocal,
};
use jni::{
    descriptors::Desc,
    errors::Error,
    objects::{GlobalRef, JClass, JObject, JObjectArray, JThrowable},
    sys::{jlong, jsize},
    JNIEnv, NativeMethod,
};
use std::{
    cell::Cell,
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, OnceLock},
    time::Instant,
};

// Maps Java invocation handler IDs to Rust closures.
// `LazyLock` is required for a const initializer.
// `Arc` is required for having `dyn` closures and using them after dropping the MutexGuard.
static RUST_HANDLERS: LazyLock<Mutex<HashMap<i64, Arc<RustHandler>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// The lifetime sugar cannot apply here, because the closure requires multiple reference
// as parameters. Reference: <https://doc.rust-lang.org/stable/nomicon/hrtb.html>.
// Requiring all references here to have the same lifetime bounds doesn't introduce
// any inconvenience outside, because these closures are called only in `rust_callback()`.
// It's tested that returning a new local reference to the Java caller doesn't leak.
type RustHandler = dyn for<'a> Fn(&mut JNIEnv<'a>, &JObject<'a>, &[&JObject<'a>]) -> Result<AutoLocal<'a>, Error>
    + Send
    + Sync;

// This indicates the invoked proxy ID for the Rust handler; it should be `None` elsewhere.
thread_local! {
    static CURRENT_PROXY_ID: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Java dynamic proxy with an invocation handler backed by the Rust closure.
///
/// It removes the Rust handler on dropping. Dropping the handler will cause problems
/// if methods with non-void returning type are still called from the Java side.
///
/// References:
/// - <https://developer.classpath.org/doc/java/lang/reflect/InvocationHandler.html>
/// - <https://docs.oracle.com/javase/8/docs/api/java/lang/reflect/InvocationHandler.html>
/// - <https://docs.oracle.com/javase/8/docs/api/java/lang/reflect/Proxy.html>
///
/// TODO: Manage to extend any abstract class (not interface), see `javassist` and `dexmaker`.
///
/// ```
/// use jni_min_helper::*;
/// jni_with_env(|env| {
///     let proxy = JniProxy::build(
///         env,
///         None,
///         &["java/util/concurrent/Callable"],
///         |env, method, args| {
///             assert_eq!(args.len(), 0);
///             format!(
///                 "Method `{}` is called with proxy {}.",
///                 method.get_method_name(env)?,
///                 JniProxy::current_proxy_id().unwrap()
///             )
///             .new_jobject(env)
///         }
///     )?;
///     let result = env
///         .call_method(&proxy, "call", "()Ljava/lang/Object;", &[])
///         .get_object(env)
///         .unwrap() // panic here if the handler returned an error
///         .get_string(env)?;
///     assert_eq!(result, format!("Method `call` is called with proxy {}.", proxy.id()));
///     
///     // Now throw an exception inside the handler
///     let _ = jni_last_cleared_ex(); // discards it
///     assert!(jni_last_cleared_ex().is_none());
///     let proxy = JniProxy::build(env, None, &["java/lang/Runnable"], |env, _, _| {
///         let s = "a".new_jobject(env)?;
///         let _ = env.call_static_method(
///             "java/lang/Integer",
///             "parseInt",
///             "(Ljava/lang/String;)I",
///             &[(&s).into()],
///         )
///         .get_int()?; // prints exception and throws it
///         JniProxy::void(env)
///     })?;
///
///     let result = env
///         .call_method(&proxy, "run", "()V", &[])
///         .map_err(jni_clear_ex_silent); // catches
///     assert!(result.is_err());
///     let last_ex = jni_last_cleared_ex().unwrap(); // takes it
///     assert!(last_ex.get_class_name(env).unwrap().contains("NumberFormatException"));
///     assert!(jni_last_cleared_ex().is_none());
///
///     // makes sure that further JNI operations still work
///     let x = jni::objects::JValue::from(-10);
///     let val = env
///         .call_static_method("java/lang/Math", "abs", "(I)I", &[x])
///         .get_int()?;
///     assert_eq!(val, 10);
///     Ok(())
/// })
/// .unwrap();
/// ```
#[derive(Debug)]
pub struct JniProxy {
    rust_hdl_id: i64,
    java_proxy: GlobalRef,
    forget: bool,
}

impl AsRef<JObject<'static>> for JniProxy {
    fn as_ref(&self) -> &JObject<'static> {
        self.java_proxy.as_obj()
    }
}

impl std::ops::Deref for JniProxy {
    type Target = JObject<'static>;
    fn deref(&self) -> &Self::Target {
        self.java_proxy.as_obj()
    }
}

impl JniProxy {
    /// Gets the proxy handler ID for debugging.
    pub fn id(&self) -> i64 {
        self.rust_hdl_id
    }

    /// Leaks the Rust handler and returns the global reference of the Java proxy.
    /// This is useful if the proxy is created for *once* in the program.
    pub fn forget(mut self) -> GlobalRef {
        self.forget = true;
        self.java_proxy.clone()
    }
}

impl Drop for JniProxy {
    fn drop(&mut self) {
        if self.forget {
            return;
        }
        if let Ok(mut hdls_locked) = RUST_HANDLERS.lock() {
            let _ = hdls_locked.remove(&self.rust_hdl_id);
        }
    }
}

impl JniProxy {
    /// Creates a Java dynamic proxy with a new invocation handler backed by the Rust closure.
    ///
    /// `class_loader` is needed if the interface definition is loaded from embeded class/dex data;
    /// in such case, `interfaces` should not be strings. Otherwise they can be strings of Java
    /// binary names (internal form) for `jni-rs` to look up them at runtime.
    ///
    /// The Rust `handler` should implement methods required by these interfaces. Primitive types
    /// have to be wrapped.
    ///
    /// Returning an error in the Rust handler function causes a Java exception to be thrown,
    /// which might be as bad as panicking in the function if the Java caller doesn't expect it.
    /// Note: the thread ID got from Rust standard library may be printed, which can be different
    /// from the ID printed by `ExceptionDescribe()` which is used on PC platforms: `Thread-0`
    /// may be `ThreadId(1)` in Rust.
    ///
    /// `equals()`, `hashCode()` and `toString()` are already implemented in the Java handler.
    pub fn build<'e, T, E, I, F>(
        env: &mut jni::JNIEnv<'e>,
        class_loader: Option<&JObject<'_>>,
        interfaces: I,
        handler: F,
    ) -> Result<Self, Error>
    where
        T: Desc<'e, JClass<'e>>,
        E: ExactSizeIterator<Item = T>,
        I: IntoIterator<Item = T, IntoIter = E>,
        F: for<'f> Fn(
                &mut JNIEnv<'f>,
                &JObject<'f>,
                &[&JObject<'f>],
            ) -> Result<AutoLocal<'f>, Error>
            + Send
            + Sync
            + 'static,
    {
        // creates a Java class array for interfaces that should be supported
        let interfaces = interfaces.into_iter();
        let arr_interfaces = env
            .new_object_array(
                interfaces.len() as jsize,
                "java/lang/Class",
                JObject::null(),
            )
            .auto_local(env)?;
        let arr_interfaces: &JObjectArray<'_> = arr_interfaces.as_ref().into();
        for (i, intr) in interfaces.enumerate() {
            let intr = intr.lookup(env)?;
            env.set_object_array_element(arr_interfaces, i as jsize, intr.as_ref())
                .map_err(jni_clear_ex)?
        }

        // creates the proxy object with a new invocation handler, register the Rust handler with its ID
        let cls_invoc_hdl: &JClass<'_> = get_invoc_hdl_class()?.into();
        let mut handlers_locked = RUST_HANDLERS.lock().unwrap();
        let id: i64 = new_hdl_id(&handlers_locked);
        let invoc_hdl = env
            .new_object(cls_invoc_hdl, "(J)V", &[id.into()])
            .auto_local(env)?;
        let proxy = env.call_static_method(
                "java/lang/reflect/Proxy",
                "newProxyInstance",
                "(Ljava/lang/ClassLoader;[Ljava/lang/Class;Ljava/lang/reflect/InvocationHandler;)Ljava/lang/Object;",
                &[
                    class_loader.unwrap_or(&JObject::null()).into(),
                    (&arr_interfaces).into(),
                    (&invoc_hdl).into()
                ]
            )
            .get_object(env)
            .globalize(env)?;
        handlers_locked.insert(id, Arc::new(handler));
        Ok(Self {
            rust_hdl_id: id,
            java_proxy: proxy,
            forget: false,
        })
    }

    /// Gets a proper void returning value for the Rust proxy handler.
    /// Note that it doesn't clear any exception being thrown.
    pub fn void<'a>(env: &JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        Ok(JObject::null()).auto_local(env)
    }

    /// Gets the invoked proxy ID inside the Rust handler closure for debugging;
    /// returns `None` elsewhere.
    pub fn current_proxy_id() -> Option<i64> {
        CURRENT_PROXY_ID.get()
    }
}

#[cfg(target_os = "android")]
impl JniProxy {
    /// Posts a `Runnable` for the Android main looper thread to do UI-related operations.
    /// Returns false on failure (usually because the looper is exiting).
    pub fn post_to_main_looper(
        runnable: impl Fn(&mut jni::JNIEnv) -> Result<(), Error> + Send + Sync + 'static,
    ) -> Result<bool, Error> {
        jni_with_env(|env| {
            // TODO: cache classes and methods used here.
            let runnable =
                JniProxy::build(env, None, ["java/lang/Runnable"], move |env, method, _| {
                    if method.get_method_name(env)? == "run" {
                        let _ = runnable(env);
                        let _ = env.exception_clear();
                    }
                    if let (Some(cur_id), Ok(mut hdls_locked)) =
                        (JniProxy::current_proxy_id(), RUST_HANDLERS.lock())
                    {
                        let _ = hdls_locked.remove(&cur_id);
                    }
                    JniProxy::void(env)
                })?;
            let main_looper = env
                .call_static_method(
                    "android/os/Looper",
                    "getMainLooper",
                    "()Landroid/os/Looper;",
                    &[],
                )
                .get_object(env)?
                .null_check_owned("android.os.Looper.getMainLooper() returned null")?;
            let handler = env
                .new_object(
                    "android/os/Handler",
                    "(Landroid/os/Looper;)V",
                    &[(&main_looper).into()],
                )
                .auto_local(env)?;
            let posted = env
                .call_method(
                    &handler,
                    "post",
                    "(Ljava/lang/Runnable;)Z",
                    &[(&runnable).into()],
                )
                .get_boolean()?;
            if posted {
                // the runnable will remove the handler by itself, when it is called for once
                let _ = runnable.forget();
            }
            Ok(posted)
        })
    }
}

fn get_invoc_hdl_class() -> Result<&'static JObject<'static>, Error> {
    static INVOC_HDL_CLASS: OnceLock<GlobalRef> = OnceLock::new();
    if INVOC_HDL_CLASS.get().is_none() {
        jni_with_env(|env| {
            let class_loader = get_helper_class_loader()?;
            let class = class_loader.load_class("rust/jniminhelper/InvocHdl")?;
            // register `rust_callback()`
            let native_method = NativeMethod {
                name: "rustHdl".into(),
                sig: "(JLjava/lang/reflect/Method;[Ljava/lang/Object;)Ljava/lang/Object;".into(),
                fn_ptr: rust_callback as *mut _,
            };
            env.register_native_methods(class.as_class(), &[native_method])
                .map_err(jni_clear_ex)?;
            let _ = INVOC_HDL_CLASS.set(class);
            Ok(())
        })?;
    }
    Ok(INVOC_HDL_CLASS.get().unwrap())
}

// Note: this function depends on `clock_gettime()` on UNIX, including Android.
fn new_hdl_id(handlers_locked: &HashMap<i64, Arc<RustHandler>>) -> i64 {
    static STARTUP_INSTANT: LazyLock<Instant> = LazyLock::new(Instant::now);
    loop {
        let nanos = STARTUP_INSTANT.elapsed().as_nanos();
        let num = (nanos % (i64::MAX as u128)) as i64;
        if !handlers_locked.contains_key(&num) {
            return num;
        }
    }
}

pub(crate) fn read_object_array<'e>(
    arr: &JObjectArray<'_>,
    env: &mut JNIEnv<'e>,
) -> Result<Vec<AutoLocal<'e>>, Error> {
    if arr.is_null() {
        return Err(Error::NullPtr("read_object_array"));
    }
    let len = env.get_array_length(arr).map_err(jni_clear_ex)?;
    let mut vec = Vec::with_capacity(len as usize);
    for i in 0..len {
        vec.push(env.get_object_array_element(arr, i).auto_local(env)?);
    }
    Ok(vec)
}

// Its local reference parameters are casted from their C counterparts,
// they don't cause memory leak problem.
extern "C" fn rust_callback<'a>(
    mut env: JNIEnv<'a>,
    _this: JObject<'a>,
    rust_hdl_id: jlong,
    method: JObject<'a>,
    args: JObjectArray<'a>,
) -> JObject<'a> {
    let lock = RUST_HANDLERS.lock().unwrap();
    let rust_hdl = if let Some(f) = (*lock).get(&rust_hdl_id) {
        f.clone()
    } else {
        warn!("Proxy {rust_hdl_id} is used, but the Rust handler has been dropped.");
        return JObject::null();
    };
    // ReentrantMutex is not needed(?) even if `rust_hdl()` registers another handler.
    drop(lock);

    let args = read_object_array(&args, &mut env).unwrap_or_default();
    let args: Vec<_> = args.iter().map(|o| o.as_ref()).collect();
    CURRENT_PROXY_ID.replace(Some(rust_hdl_id));

    let result = rust_hdl(&mut env, &method, &args);

    let _ = CURRENT_PROXY_ID.take();
    match result {
        Ok(obj) => obj.forget(),
        Err(Error::JavaException) => {
            let th = std::thread::current().id();
            if !env.exception_check().unwrap() {
                if let Some(ex) = jni_last_cleared_ex() {
                    // it was cleared by `jni_clear_ex()`, throw it again
                    warn!(
                        "{th:?}: Rust handler of proxy {rust_hdl_id} got an exception, throwing..."
                    );
                    let ex = env.new_local_ref(&ex).unwrap();
                    env.throw(JThrowable::from(ex)) // tested: it doesn't cause memory leak here
                } else {
                    // it was cleared by some other mean in the closure
                    env.throw("{th:?}: Rust handler of proxy {rust_hdl_id} got an exception.")
                }
                .unwrap();
            } // or else let it throw the exception automatically
            JObject::null()
        }
        Err(e) => {
            let th = std::thread::current().id();
            env.throw(format!(
                "{th:?}: Rust handler of proxy {rust_hdl_id}: {e:?}",
            ))
            .unwrap();
            JObject::null()
        }
    }
}
