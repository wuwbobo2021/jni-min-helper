use crate::{
    jni_with_env,
    receiver::{AndroidBroadcastReceiver, Intent, IntentFilter},
};
use jni::{
    Env, bind_java_type,
    errors::Error,
    jni_sig, jni_str,
    objects::{JClassLoader, JObject, JString},
    refs::Global,
};

use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::OnceLock,
};

const DEX_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classes.dex"));

pub(crate) fn get_helper_class_loader() -> Result<&'static JClassLoader<'static>, Error> {
    static CLASS_LOADER: OnceLock<Global<JClassLoader<'static>>> = OnceLock::new();
    if CLASS_LOADER.get().is_none() {
        let loader = jni_with_env(|env| {
            let dex_loader = get_android_context()
                .get_class_loader(env)?
                .load_dex(env, DEX_DATA)?;
            env.new_global_ref(dex_loader)
        })?;
        let _ = CLASS_LOADER.set(loader);
    }
    Ok(CLASS_LOADER.get().unwrap())
}

bind_java_type! {
    pub(crate) AndroidContext => "android.content.Context",
    type_map = {
        JFile => "java.io.File",
        AndroidBroadcastReceiver => "android.content.BroadcastReceiver",
        Intent => "android.content.Intent",
        IntentFilter => "android.content.IntentFilter",
    },
    methods {
        fn get_files_dir() -> JFile,
        fn get_cache_dir() -> JFile,
        fn get_code_cache_dir() -> JFile, // API level >= 21
        fn get_dir(name: JString, mode: jint) -> JFile,
        fn get_class_loader() -> JClassLoader,
        fn get_package_name() -> JString,
        fn register_receiver {
            name = "registerReceiver",
            sig = (receiver: AndroidBroadcastReceiver, filter: IntentFilter) -> Intent,
        },
        fn unregister_receiver(receiver: AndroidBroadcastReceiver),
        fn check_self_permission(permission: JString) -> jint,
        fn start_activity(intent: Intent) -> (),
    }
}

bind_java_type! {
    pub(crate) JFile => "java.io.File",
    methods {
        fn get_absolute_path() -> JString,
    }
}

bind_java_type! {
    InMemoryDexClassLoader => "dalvik.system.InMemoryDexClassLoader",
    constructors {
        fn new(dex_buffer: JByteBuffer, parent: JClassLoader),
    },
    is_instance_of = {
        JClassLoader,
    }
}

bind_java_type! {
    DexFileClassLoader => "dalvik.system.DexClassLoader",
    constructors {
        fn new(dex_path: JString, optimized_directory: JString, library_search_path: JString, parent: JClassLoader),
    },
    is_instance_of = {
        JClassLoader,
    }
}

bind_java_type! {
    AndroidBuildVersion => "android.os.Build$VERSION",
    fields {
        #[allow(non_snake_case)]
        static SDK_INT {
            sig = jint,
            get = SDK_INT,
        },
    },
}

/// Provides DEX class loading support for Android.
pub trait DexClassLoader<'local> {
    /// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at compile time,
    /// having the current loader as the parent loader. This function may do heavy operations.
    fn load_dex(
        &self,
        env: &mut Env<'local>,
        dex_data: &'static [u8],
    ) -> Result<JClassLoader<'local>, Error>;
}

impl<'local> DexClassLoader<'local> for JClassLoader<'local> {
    /// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at compile time,
    /// having the current loader as the parent loader. This function may do heavy operations.
    fn load_dex(
        &self,
        env: &mut Env<'local>,
        dex_data: &'static [u8],
    ) -> Result<JClassLoader<'local>, Error> {
        let context = get_android_context();
        if android_api_level() >= 26 {
            // Safety: dex_data is 'static and the `InMemoryDexClassLoader`` will not mutate it.
            // The data may be converted by `ConvertDexFilesToJavaArray()` and handled by the
            // created Java class loader, which shouldn't be freed before the class and its
            // objects are freed. So this local reference doesn't need to be leaked.
            let dex_buffer =
                unsafe { env.new_direct_byte_buffer(dex_data.as_ptr() as *mut _, dex_data.len()) }?;
            let dex_loader = InMemoryDexClassLoader::new(env, &dex_buffer, self)?;
            JClassLoader::cast_local(env, dex_loader)
        } else {
            // The dex data must be written in a file; this determines the output
            // directory path inside the application code cache directory.
            let code_cache_path = context
                .get_code_cache_dir(env)?
                .get_absolute_path(env)
                .map(|p| std::path::PathBuf::from(p.to_string()))?;

            // Creates the dex file. before creating, calculate the hash for a unique dex name, which
            // may determine names of oat files, which may be mapped to the virtual memory for execution.
            let dex_hash = {
                use std::hash::{DefaultHasher, Hasher};
                let mut hasher = DefaultHasher::new();
                hasher.write(dex_data);
                hasher.finish()
            };
            let dex_name = format!("{dex_hash:016x}.dex");
            let dex_file_path = code_cache_path.join(dex_name);
            std::fs::write(&dex_file_path, dex_data).unwrap(); // Note: this panics on failure
            let dex_file_path = JString::new(env, dex_file_path.to_string_lossy())?;

            // creates the oats directory
            let oats_dir_path = code_cache_path.join("oats");
            let _ = std::fs::create_dir(&oats_dir_path);
            let oats_dir_path = JString::new(env, oats_dir_path.to_string_lossy())?;

            // loads the dex file
            let dex_loader = DexFileClassLoader::new(
                env,
                &dex_file_path,
                &oats_dir_path,
                JString::null(),
                self,
            )?;
            JClassLoader::cast_local(env, dex_loader)
        }
    }
}

/// Gets the current `android.content.Context`, usually a reference of `NativeActivity`.
/// This depends on crate `ndk_context`.
pub fn android_context() -> &'static JObject<'static> {
    get_android_context().as_ref()
}

pub(crate) fn get_android_context() -> &'static AndroidContext<'static> {
    static ANDROID_CONTEXT: OnceLock<(Global<AndroidContext<'static>>, bool)> = OnceLock::new();
    let (ctx, from_glue_crate) = ANDROID_CONTEXT.get_or_init(|| {
        jni_with_env(|env| {
            let ctx = ndk_context::android_context();
            // Safety: as documented in `cargo-apk` example to obtain the context's JNI reference.
            // It's set by `android_activity`, got from `ANativeActivity_onCreate()` entry, and it
            // can be used across threads, thus it should be a global reference by itself.
            let obj = unsafe { AndroidContext::from_raw(env, ctx.context().cast()) };
            if !obj.is_null() {
                Ok((env.new_global_ref(obj)?, true))
            } else {
                let th = get_activity_thread(env)?;
                let app = env
                    .call_method(
                        &th,
                        jni_str!("getApplication"),
                        jni::jni_sig!(() -> android.app.Application),
                        &[],
                    )?
                    .l()?;
                let ctx = AndroidContext::cast_local(env, app)?;
                let ctx = env.new_global_ref(ctx)?;
                if ctx.is_null() {
                    panic!("got null from ActivityThread.getApplication()");
                }
                Ok((ctx, false))
            }
        })
        .unwrap()
    });
    if !from_glue_crate {
        // `warn!` doesn't work inside the closure for `get_or_init()`.
        warn!("`ndk_context::android_context().context()` is null. Check the Android glue crate.");
        warn!("Using `Application` (No `Activity` and UI availability); other crates may fail.");
    }
    ctx.as_ref()
}

fn get_activity_thread<'a>(env: &mut Env<'a>) -> Result<JObject<'a>, Error> {
    env.call_static_method(
        jni_str!("android/app/ActivityThread"),
        jni_str!("currentActivityThread"),
        jni_sig!(() -> Landroid.app.ActivityThread),
        &[],
    )?
    .l()
}

/// Gets the API level (SDK version) of the current Android OS.
pub fn android_api_level() -> i32 {
    static API_LEVEL: OnceLock<i32> = OnceLock::new();
    *API_LEVEL.get_or_init(|| jni_with_env(|env| AndroidBuildVersion::SDK_INT(env)).unwrap())
}

/// Gets the raw name of the current Android application, parsed from the package name.
pub fn android_app_name() -> &'static str {
    static APP_NAME: OnceLock<String> = OnceLock::new();
    APP_NAME.get_or_init(|| {
        android_app_package_name()
            .split('.')
            .next_back()
            .unwrap()
            .to_string()
    })
}

/// Gets the package name of the current Android application.
pub fn android_app_package_name() -> &'static str {
    static PACKAGE_NAME: OnceLock<String> = OnceLock::new();
    PACKAGE_NAME.get_or_init(|| {
        jni_with_env(|env| {
            get_android_context()
                .get_package_name(env)
                .map(|s| s.to_string())
        })
        .unwrap()
    })
}

/// Returns the absolute path to the directory holding application files. No permissions
/// are required for the calling app to read or write files under the returned path.
pub fn android_app_files_dir() -> &'static Path {
    static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
    FILES_DIR.get_or_init(|| {
        jni_with_env(|env| {
            get_android_context()
                .get_files_dir(env)?
                .get_absolute_path(env)
                .map(|s| PathBuf::from_str(&s.to_string()).unwrap())
        })
        .unwrap()
    })
}

/// Returns the absolute path to the application specific cache directory.
pub fn android_app_cache_dir() -> &'static Path {
    static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
    CACHE_DIR.get_or_init(|| {
        jni_with_env(|env| {
            get_android_context()
                .get_cache_dir(env)?
                .get_absolute_path(env)
                .map(|s| PathBuf::from_str(&s.to_string()).unwrap())
        })
        .unwrap()
    })
}
