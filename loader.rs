use crate::{convert::*, jni_clear_ex_ignore, jni_with_env, AutoLocalGlobalize, JObjectAutoLocal};
use jni::{errors::Error, objects::*};

#[allow(unused)]
use std::sync::OnceLock;

#[cfg(feature = "proxy")]
#[cfg(not(target_os = "android"))]
const CLASS_DATA: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/rust/jniminhelper/InvocHdl.class"
));

#[cfg(feature = "proxy")]
#[cfg(target_os = "android")]
const DEX_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classes.dex"));

#[cfg(feature = "proxy")]
pub(crate) fn get_helper_class_loader() -> Result<&'static JniClassLoader, Error> {
    static CLASS_LOADER: OnceLock<JniClassLoader> = OnceLock::new();
    #[cfg(not(target_os = "android"))]
    if CLASS_LOADER.get().is_none() {
        let loader = JniClassLoader::app_loader()?;
        loader.define_class("rust/jniminhelper/InvocHdl", CLASS_DATA)?;
        let _ = CLASS_LOADER.set(loader);
    }
    #[cfg(target_os = "android")]
    if CLASS_LOADER.get().is_none() {
        let loader = JniClassLoader::load_dex(DEX_DATA)?;
        let _ = CLASS_LOADER.set(loader);
    }
    Ok(CLASS_LOADER.get().unwrap())
}

/// Runtime class data loader. Wraps a global reference of `java.lang.ClassLoader`.
#[derive(Clone, Debug)]
pub struct JniClassLoader {
    inner: GlobalRef,
}

impl TryFrom<&JObject<'_>> for JniClassLoader {
    type Error = Error;
    fn try_from(value: &JObject<'_>) -> Result<Self, Self::Error> {
        jni_with_env(|env| {
            let cls_loader = env.find_class("java/lang/ClassLoader").auto_local(env)?;
            value
                .class_check(cls_loader.as_class(), env)
                .and_then(|l| env.new_global_ref(l))
                .map(|inner| Self { inner })
        })
    }
}

impl AsRef<JObject<'static>> for JniClassLoader {
    fn as_ref(&self) -> &JObject<'static> {
        self.inner.as_obj()
    }
}

impl std::ops::Deref for JniClassLoader {
    type Target = JObject<'static>;
    fn deref(&self) -> &Self::Target {
        self.inner.as_obj()
    }
}

impl JniClassLoader {
    /// Get the context class loader via `getSystemClassLoader()`.
    #[cfg(not(target_os = "android"))]
    pub fn app_loader() -> Result<Self, Error> {
        jni_with_env(|env| {
            env.call_static_method(
                "java/lang/ClassLoader",
                "getSystemClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .get_object(env)
            .globalize(env)
            .map(|inner| Self { inner })
        })
    }

    /// Get the class loader from the current Android context.
    #[cfg(target_os = "android")]
    pub fn app_loader() -> Result<Self, Error> {
        jni_with_env(|env| {
            let context = android_context();
            env.call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
                .get_object(env)
                .globalize(env)
                .map(|inner| Self { inner })
        })
    }

    /// Loads a class of given binary name, returns a global reference of its
    /// `java.lang.Class` object. It tries `JNIEnv::find_class()` at first.
    pub fn load_class(&self, name: &str) -> Result<GlobalRef, Error> {
        jni_with_env(|env| {
            // Note: not doing this shouldn't introduce any runtime error.
            if let Ok(cls) = env
                .find_class(class_name_to_internal(name))
                .map_err(jni_clear_ex_ignore)
                .global_ref(env)
            {
                return Ok(cls);
            }

            let class_name = class_name_to_java(name).new_jobject(env)?;
            env.call_method(
                self,
                "findClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[(&class_name).into()],
            )
            .get_object(env)
            .and_then(|cls| cls.null_check_owned("ClassLoader.findClass() returned null"))
            .globalize(env)
        })
    }

    /// Loads a class of given binary name from the class file embeded at compile time,
    /// returns a JNI global reference of its `java.lang.Class` object.
    ///
    /// Not available on Android, which does not use Java bytecodes or class files.
    #[cfg(not(target_os = "android"))]
    pub fn define_class(&self, name: &str, data: &[u8]) -> Result<GlobalRef, Error> {
        jni_with_env(|env| {
            env.define_class(name, self, data)
                .global_ref(env)
                .and_then(|cls| cls.null_check_owned("JNIEnv::define_class() returned null"))
        })
    }
}

#[cfg(target_os = "android")]
impl JniClassLoader {
    /// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at
    /// compile time. This function may do heavy operations.
    pub fn load_dex(dex_data: &'static [u8]) -> Result<Self, Error> {
        // required before API level 29
        let parent_class_loader = Self::app_loader()?;
        // create the new class loader
        parent_class_loader.append_dex(dex_data)
    }

    /// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at compile time,
    /// having the current loader as the parent loader. This function may do heavy operations.
    pub fn append_dex(&self, dex_data: &'static [u8]) -> Result<Self, Error> {
        jni_with_env(|env| {
            let context = android_context();

            if android_api_level() >= 26 {
                // Safety: dex_data is 'static and the `InMemoryDexClassLoader`` will not mutate it.
                // The data may be converted by `ConvertDexFilesToJavaArray()` and handled by the
                // created Java class loader, which shouldn't be freed before the class and its
                // objects are freed. So this local reference doesn't need to be leaked.
                let dex_buffer = unsafe {
                    env.new_direct_byte_buffer(dex_data.as_ptr() as *mut _, dex_data.len())
                        .auto_local(env)?
                };
                env.new_object(
                    "dalvik/system/InMemoryDexClassLoader",
                    "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
                    &[(&dex_buffer).into(), self.into()],
                )
            } else {
                // The dex data must be written in a file; this determines the output
                // directory path inside the application code cache directory.
                let code_cache_path = if android_api_level() >= 21 {
                    env.call_method(context, "getCodeCacheDir", "()Ljava/io/File;", &[])
                } else {
                    let dir_name = "code_cache".new_jobject(env)?;
                    // create if needed
                    env.call_method(
                        context,
                        "getDir",
                        "(Ljava/lang/String;I)Ljava/io/File;",
                        &[(&dir_name).into(), 0.into()],
                    )
                }
                .get_object(env)
                .and_then(|p| env.call_method(&p, "getAbsolutePath", "()Ljava/lang/String;", &[]))
                .get_object(env)?
                .get_string(env)
                .map(std::path::PathBuf::from)?;

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
                let dex_file_path = dex_file_path.to_string_lossy().new_jobject(env)?;

                // creates the oats directory
                let oats_dir_path = code_cache_path.join("oats");
                let _ = std::fs::create_dir(&oats_dir_path);
                let oats_dir_path = oats_dir_path.to_string_lossy().new_jobject(env)?;

                // loads the dex file
                env.new_object(
                    "dalvik/system/DexClassLoader",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/ClassLoader;)V",
                    &[
                        (&dex_file_path).into(),
                        (&oats_dir_path).into(),
                        (&JObject::null()).into(),
                        self.into(),
                    ],
                )
            }
            .global_ref(env)
            .map(|inner| Self { inner })
        })
    }
}

/// Gets the current `android.content.Context`, usually a reference of `NativeActivity`.
/// This depends on crate `ndk_context`.
#[cfg(target_os = "android")]
#[inline(always)]
pub fn android_context() -> &'static JObject<'static> {
    static ANDROID_CONTEXT: OnceLock<(GlobalRef, bool)> = OnceLock::new();
    let (ctx, from_glue_crate) = ANDROID_CONTEXT.get_or_init(|| {
        jni_with_env(|env| {
            let ctx = ndk_context::android_context();
            // Safety: as documented in `cargo-apk` example to obtain the context's JNI reference.
            // It's set by `android_activity`, got from `ANativeActivity_onCreate()` entry, and it
            // can be used across threads, thus it should be a global reference by itself.
            let obj = unsafe { JObject::from_raw(ctx.context().cast()) };
            if !obj.is_null() {
                Ok((env.new_global_ref(obj)?, true))
            } else {
                let at = env
                    .call_static_method(
                        "android/app/ActivityThread",
                        "currentActivityThread",
                        "()Landroid/app/ActivityThread;",
                        &[],
                    )
                    .get_object(env)?;
                env.call_method(at, "getApplication", "()Landroid/app/Application;", &[])
                    .get_object(env)
                    .globalize(env)
                    .map(|ctx| (ctx, false))
            }
        })
        .unwrap()
    });
    if !from_glue_crate {
        // `warn!` doesn't work inside the closure for `get_or_init()`.
        warn!("`ndk_context::android_context().context()` is null. Check the Android glue crate.");
        warn!("Using `Application` (No `Activity` and UI availability); other crates may fail.");
    }
    ctx.as_obj()
}

/// Gets the API level (SDK version) of the current Android OS.
#[cfg(target_os = "android")]
pub fn android_api_level() -> i32 {
    static API_LEVEL: OnceLock<i32> = OnceLock::new();
    *API_LEVEL.get_or_init(|| {
        jni_with_env(|env| {
            // the version can be read from `android_activity` or `ndk_sys`,
            // but here it tries to avoid such dependency or making unsafe calls.
            let os_build_class = env.find_class("android/os/Build$VERSION").unwrap();
            env.get_static_field(&os_build_class, "SDK_INT", "I")
                .get_int()
        })
        .unwrap()
    })
}

/// Gets the raw name of the current Android application, parsed from the package name.
#[cfg(target_os = "android")]
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
#[cfg(target_os = "android")]
pub fn android_app_package_name() -> &'static str {
    static PACKAGE_NAME: OnceLock<String> = OnceLock::new();
    PACKAGE_NAME.get_or_init(|| {
        jni_with_env(|env| {
            let ctx = android_context();
            env.call_method(ctx, "getPackageName", "()Ljava/lang/String;", &[])
                .get_object(env)?
                .get_string(env)
        })
        .unwrap()
    })
}
