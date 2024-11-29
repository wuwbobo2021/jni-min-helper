use crate::{convert::*, jni_clear_ex_ignore, jni_last_cleared_ex, JObjectAutoLocal};
use jni::{errors::Error, objects::*, AttachGuard, JavaVM};
use std::sync::OnceLock;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Attaches the current thread to the JVM after `jni_get_vm()`.
///
/// Reference:
/// <https://docs.rs/jni/latest/jni/struct.JavaVM.html#method.attach_current_thread>
#[inline(always)]
pub fn jni_attach_vm<'a>() -> Result<AttachGuard<'a>, Error> {
    jni_get_vm().attach_current_thread()
}

/// Tells this crate to use an existing JVM, instead of launching a new JVM
/// with no arguments (which may panic on failure). Not available on Android.
#[cfg(not(target_os = "android"))]
pub fn jni_set_vm(vm: &JavaVM) -> bool {
    if JAVA_VM.get().is_some() {
        false
    } else {
        // Safety: #[derive(Clone)] is to be added for struct JavaVM(*mut sys::JavaVM),
        // also check the source code of JNIEnv::get_java_vm().
        let vm = unsafe { JavaVM::from_raw(vm.get_java_vm_pointer()).unwrap() };
        JAVA_VM.set(vm).unwrap();
        true
    }
}

/// Gets the remembered `JavaVM`, otherwise it launches a new JVM with no arguments
/// (which may panic on failure).
#[cfg(not(target_os = "android"))]
#[inline(always)]
pub fn jni_get_vm() -> &'static JavaVM {
    JAVA_VM.get_or_init(|| {
        let args = jni::InitArgsBuilder::new().build().unwrap();
        JavaVM::new(args).unwrap()
    })
}

/// Gets the `JavaVM` from current Android context.
#[cfg(target_os = "android")]
#[inline(always)]
pub fn jni_get_vm() -> &'static JavaVM {
    JAVA_VM.get_or_init(|| {
        let ctx = ndk_context::android_context();
        // Safety: as documented in `ndk-context` to obtain the `jni::JavaVM`
        unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap()
    })
}

/// Gets the current `android.content.Context`, usually a reference of `NativeActivity`.
#[cfg(target_os = "android")]
#[inline(always)]
pub fn jni_android_context() -> &'static JObject<'static> {
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
        let ctx = jni_android_context();
        env.call_method(ctx, "getPackageName", "()Ljava/lang/String;", &[])
            .get_object(env)
            .unwrap()
            .get_string(env)
            .unwrap()
            .unwrap()
    })
}

/// Loads a class of given binary name from the class file embeded at compile time,
/// returns a JNI global reference of its `java.lang.Class` object.
#[cfg(not(target_os = "android"))]
pub fn jni_load_class_from_data(data: &'static [u8], name: &str) -> Result<GlobalRef, Error> {
    let env = &mut jni_attach_vm()?;
    let name = class_name_to_internal(name);
    if let Ok(cls) = env
        .find_class(&name)
        .map_err(jni_clear_ex_ignore)
        .global_ref(env)
    {
        return Ok(cls);
    }
    let _ = jni_last_cleared_ex();
    let loader = env
        .call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .get_object(env)?;
    env.define_class(&name, &loader, data).global_ref(env)
}

/// Loads a class of given binary name from existing class loaders, or from
/// the dex file data embeded at compile time. Note: Android does not use
/// Java bytecodes or class files; this function may do heavy operations
/// before returning a JNI global reference of the `java.lang.Class` object.
#[cfg(target_os = "android")]
pub fn jni_load_class_from_dex(dex_data: &'static [u8], name: &str) -> Result<GlobalRef, Error> {
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

    let class_loader = jni_create_dex_class_loader(dex_data)?;
    jni_load_class_with_loader(class_loader.as_obj(), name)
}

/// Loads a class of given binary name with the provided `java.lang.ClassLoader`,
#[cfg(target_os = "android")]
pub fn jni_load_class_with_loader(
    class_loader: &JObject<'_>,
    name: &str,
) -> Result<GlobalRef, Error> {
    use crate::AutoLocalGlobalize;
    let env = &mut jni_attach_vm()?;
    let class_name = class_name_to_java(name).create_jobject(env)?;
    env.call_method(
        class_loader,
        "findClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[(&class_name).into()],
    )
    .get_object(env)
    .globalize(env)
}

/// Creates a `dalvik.system.DexClassLoader` from given dex file data embeded at
/// compile time. This function may do heavy operations. Use this function with
/// `jni_load_class_with_loader()` for loading multiple classes.
#[cfg(target_os = "android")]
pub fn jni_create_dex_class_loader(dex_data: &'static [u8]) -> Result<GlobalRef, Error> {
    let env = &mut jni_attach_vm()?;
    let context = jni_android_context();

    // required before API level 29
    let parent_class_loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .get_object(env)?;

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
        .global_ref(env)
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
            let dir_name = "code_cache".create_jobject(env)?;
            env.call_method(
                context,
                "getDir",
                "(Ljava/lang/String;I)Ljava/io/File;",
                &[(&dir_name).into(), 0.into()],
            )
            .get_object(env)?
        };
        // generates the dex file path.
        let dex_name = format!("{:016x}.dex", dex_hash).create_jobject(env)?;
        let dex_file_path = env
            .new_object(
                "java/io/File",
                "(Ljava/io/File;Ljava/lang/String;)V",
                &[(&code_cache_path).into(), (&dex_name).into()],
            )
            .auto_local(env)?;
        // creates the oats directory
        let oats_dir_name = "oats".create_jobject(env)?;
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
        .global_ref(env)
    }
}
