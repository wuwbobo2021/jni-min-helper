use crate::{
    convert::*, jni_attach_vm, jni_clear_ex_ignore, jni_last_cleared_ex, AutoLocalGlobalize,
    JObjectAutoLocal,
};
use jni::{errors::Error, objects::*};
use std::sync::OnceLock;

#[cfg(not(target_os = "android"))]
const CLASS_DATA: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/rust/jniminhelper/InvocHdl.class"
));

#[cfg(target_os = "android")]
const DEX_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classes.dex"));

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
        let env = &mut jni_attach_vm()?;
        let cls_loader = env.find_class("java/lang/ClassLoader").auto_local(env)?;
        value
            .class_check(cls_loader.as_class(), "JniClassLoader::try_from", env)
            .and_then(|l| env.new_global_ref(l))
            .map(|inner| Self { inner })
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
        let env = &mut jni_attach_vm()?;
        env.call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .get_object(env)
        .globalize(env)
        .map(|inner| Self { inner })
    }

    /// Get the class loader from the current Android context.
    #[cfg(target_os = "android")]
    pub fn app_loader() -> Result<Self, Error> {
        let env = &mut jni_attach_vm()?;
        let context = android_context();
        env.call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .get_object(env)
            .globalize(env)
            .map(|inner| Self { inner })
    }

    /// Loads a class of given binary name, returns a global reference of its
    /// `java.lang.Class` object. It tries `JNIEnv::find_class()` at first.
    pub fn load_class(&self, name: &str) -> Result<GlobalRef, Error> {
        let env = &mut jni_attach_vm()?;

        // Note: not doing this shouldn't introduce any runtime error.
        if let Ok(cls) = env
            .find_class(class_name_to_internal(name))
            .map_err(jni_clear_ex_ignore)
            .global_ref(env)
        {
            return Ok(cls);
        }
        let _ = jni_last_cleared_ex();

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
    }

    /// Loads a class of given binary name from the class file embeded at compile time,
    /// returns a JNI global reference of its `java.lang.Class` object.
    ///
    /// Not available on Android, which does not use Java bytecodes or class files.
    #[cfg(not(target_os = "android"))]
    pub fn define_class(&self, name: &str, data: &[u8]) -> Result<GlobalRef, Error> {
        let env = &mut jni_attach_vm()?;
        env.define_class(name, self, data)
            .global_ref(env)
            .and_then(|cls| cls.null_check_owned("JNIEnv::define_class() returned null"))
    }
}

#[cfg(target_os = "android")]
impl JniClassLoader {
    /// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at
    /// compile time. This function may do heavy operations.
    pub fn load_dex(dex_data: &'static [u8]) -> Result<Self, Error> {
        let env = &mut jni_attach_vm()?;
        let context = android_context();

        // required before API level 29
        let parent_class_loader = Self::app_loader()?;

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
                &[(&dex_buffer).into(), (&parent_class_loader).into()],
            )
        } else {
            // the dex data must be written in a file
            let dex_byte_array = env.byte_array_from_slice(dex_data).auto_local(env)?;
            let dex_data_len = dex_data.len() as i32;
            // calculates the hash for a unique dex name, which may determine the name of
            // the oat file, which may be mapped to the virtual memory for execution.
            let dex_hash = {
                use std::hash::{DefaultHasher, Hasher};
                let mut hasher = DefaultHasher::new();
                hasher.write(dex_data);
                hasher.finish()
            };
            // determines the code cache dir path in the application code cache directory
            let code_cache_path = if android_api_level() >= 21 {
                env.call_method(context, "getCodeCacheDir", "()Ljava/io/File;", &[])
                    .get_object(env)?
            } else {
                let dir_name = "code_cache".new_jobject(env)?;
                env.call_method(
                    context,
                    "getDir",
                    "(Ljava/lang/String;I)Ljava/io/File;",
                    &[(&dir_name).into(), 0.into()],
                )
                .get_object(env)?
            };
            // generates the dex file path.
            let dex_name = format!("{:016x}.dex", dex_hash).new_jobject(env)?;
            let dex_file_path = env
                .new_object(
                    "java/io/File",
                    "(Ljava/io/File;Ljava/lang/String;)V",
                    &[(&code_cache_path).into(), (&dex_name).into()],
                )
                .auto_local(env)?;
            // creates the oats directory
            let oats_dir_name = "oats".new_jobject(env)?;
            let oats_dir_path = env
                .new_object(
                    "java/io/File",
                    "(Ljava/io/File;Ljava/lang/String;)V",
                    &[(&code_cache_path).into(), (&oats_dir_name).into()],
                )
                .auto_local(env)?;
            let _ = env
                .call_method(&oats_dir_path, "mkdir", "()Z", &[])
                .get_boolean()?;
            // turns them to Java string
            let dex_file_path = env
                .call_method(
                    dex_file_path,
                    "getAbsolutePath",
                    "()Ljava/lang/String;",
                    &[],
                )
                .get_object(env)?;
            let oats_dir_path = env
                .call_method(
                    oats_dir_path,
                    "getAbsolutePath",
                    "()Ljava/lang/String;",
                    &[],
                )
                .get_object(env)?;

            // writes the dex data
            let write_stream = env
                .new_object(
                    "java/io/FileOutputStream",
                    "(Ljava/lang/String;)V",
                    &[(&dex_file_path).into()],
                )
                .auto_local(env)?;
            env.call_method(
                &write_stream,
                "write",
                "([BII)V",
                &[(&dex_byte_array).into(), 0.into(), dex_data_len.into()],
            )
            .clear_ex()?;
            env.call_method(&write_stream, "close", "()V", &[]).unwrap();

            // loads the dex file
            env.new_object(
                "dalvik/system/DexClassLoader",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/ClassLoader;)V",
                &[
                    (&dex_file_path).into(),
                    (&oats_dir_path).into(),
                    (&JObject::null()).into(),
                    (&parent_class_loader).into(),
                ],
            )
        }
        .global_ref(env)
        .map(|inner| Self { inner })
    }
}

/// Gets the current `android.content.Context`, usually a reference of `NativeActivity`.
#[cfg(target_os = "android")]
#[inline(always)]
pub fn android_context() -> &'static JObject<'static> {
    static ANDROID_CONTEXT: OnceLock<GlobalRef> = OnceLock::new();
    ANDROID_CONTEXT
        .get_or_init(|| {
            let env = &mut jni_attach_vm().unwrap();
            let ctx = ndk_context::android_context();
            // Safety: as documented in `cargo-apk` example to obtain the context's JNI reference.
            // It's set by `android_activity`, got from `ANativeActivity_onCreate()` entry, and it
            // can be used across threads, thus it should be a global reference by itself.
            let obj = unsafe { JObject::from_raw(ctx.context().cast()) };
            env.new_global_ref(obj).unwrap()
        })
        .as_obj()
}

/// Gets the API level (SDK version) of the current Android OS.
#[cfg(target_os = "android")]
pub fn android_api_level() -> i32 {
    static API_LEVEL: OnceLock<i32> = OnceLock::new();
    *API_LEVEL.get_or_init(|| {
        let env = &mut jni_attach_vm().unwrap();
        // the version can be read from `android_activity` or `ndk_sys`,
        // but here it tries to avoid such dependency or making unsafe calls.
        let os_build_class = env.find_class("android/os/Build$VERSION").unwrap();
        env.get_static_field(&os_build_class, "SDK_INT", "I")
            .get_int()
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
            .last()
            .unwrap()
            .to_string()
    })
}

/// Gets the package name of the current Android application.
#[cfg(target_os = "android")]
pub fn android_app_package_name() -> &'static str {
    static PACKAGE_NAME: OnceLock<String> = OnceLock::new();
    PACKAGE_NAME.get_or_init(|| {
        let env = &mut jni_attach_vm().unwrap();
        let ctx = android_context();
        env.call_method(ctx, "getPackageName", "()Ljava/lang/String;", &[])
            .get_object(env)
            .unwrap()
            .get_string(env)
            .unwrap()
    })
}
