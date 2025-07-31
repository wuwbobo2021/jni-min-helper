use std::sync::{Mutex, OnceLock};

#[cfg(not(feature = "futures"))]
use std::sync::mpsc::{channel, Receiver, Sender};

#[cfg(feature = "futures")]
use futures_channel::oneshot::{channel, Receiver, Sender};

use crate::{
    convert::*,
    jni_clear_ex, jni_with_env,
    loader::{android_api_level, android_context, get_helper_class_loader},
    proxy::read_object_array,
    JObjectAutoLocal,
};

use jni::{
    errors::Error,
    objects::{GlobalRef, JIntArray, JObject, JObjectArray},
    sys::jsize,
    JNIEnv, NativeMethod,
};

const PERMISSION_GRANTED: i32 = 0;
const EXTRA_PERM_ARRAY: &str = "rust.jniminhelper.perm_array";
const EXTRA_TITLE: &str = "rust.jniminhelper.perm_activity_title";

type RequestResult = Vec<(String, bool)>;

static MUTEX_PERM_REQ: Mutex<Option<Sender<RequestResult>>> = Mutex::new(None);

/// Android runtime permission (introduced in Android 6.0, API level 23) request utility.
pub struct PermissionRequest {
    receiver: Receiver<RequestResult>,
}

impl PermissionRequest {
    /// Checks if a permission is already granted.
    /// Returns `Error::MethodNotFound` if the Android API level is less than 23.
    pub fn has_permission(permission: &str) -> Result<bool, Error> {
        if android_api_level() < 23 {
            return Err(Error::MethodNotFound {
                name: "checkSelfPermission".to_string(),
                sig: "Android API level < 23".to_string(),
            });
        }
        jni_with_env(|env| {
            let context = android_context();
            let permission = permission.new_jobject(env)?;
            env.call_method(
                context,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[(&permission).into()],
            )
            .get_int()
            .map(|i| i == PERMISSION_GRANTED)
        })
    }

    /// Returns true if there is an ongoing request managed by this crate.
    pub fn is_pending() -> bool {
        MUTEX_PERM_REQ.lock().unwrap().is_some()
    }

    /// Starts a permission request for permission names listed in `permissions`.
    /// Returns `Error::TryLock` if a previous requested in unfinished;
    /// returns `Ok(None)` if the Android API level is less than 23.
    pub fn request<'a>(
        title: &str,
        permissions: impl IntoIterator<Item = &'a str>,
    ) -> Result<Option<Self>, Error> {
        if android_api_level() < 23 {
            return Ok(None);
        }
        if Self::is_pending() {
            return Err(Error::TryLock);
        }

        let mut perms = Vec::new();
        for perm in permissions.into_iter() {
            if !Self::has_permission(perm)? {
                perms.push(perm.to_string());
            }
        }
        if perms.is_empty() {
            return Ok(None);
        }

        let receiver = jni_with_env(|env| {
            let context = android_context();

            let intent = env
                .new_object("android/content/Intent", "()V", &[])
                .auto_local(env)?;

            let cls_perm = get_perm_activity_class()?;
            env.call_method(
                &intent,
                "setClass",
                "(Landroid/content/Context;Ljava/lang/Class;)Landroid/content/Intent;",
                &[context.into(), cls_perm.into()],
            )
            .clear_ex()?;

            let extra_title = EXTRA_TITLE.new_jobject(env)?;
            let title = title.new_jobject(env)?;
            env.call_method(
                &intent,
                "putExtra",
                "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
                &[(&extra_title).into(), (&title).into()],
            )
            .clear_ex()?;

            let arr_perms = env
                .new_object_array(perms.len() as jsize, "java/lang/String", JObject::null())
                .auto_local(env)?;
            let arr_perms: &JObjectArray<'_> = arr_perms.as_ref().into();
            for (i, perm) in perms.iter().enumerate() {
                let perm = perm.new_jobject(env)?;
                env.set_object_array_element(arr_perms, i as jsize, &perm)
                    .map_err(jni_clear_ex)?;
            }
            let extra_perm_array = EXTRA_PERM_ARRAY.new_jobject(env)?;
            env.call_method(
                &intent,
                "putExtra",
                "(Ljava/lang/String;[Ljava/lang/CharSequence;)Landroid/content/Intent;",
                &[(&extra_perm_array).into(), (&arr_perms).into()],
            )
            .clear_ex()?;

            let (tx, rx) = channel();
            MUTEX_PERM_REQ.lock().unwrap().replace(tx);

            env.call_method(
                context,
                "startActivity",
                "(Landroid/content/Intent;)V",
                &[(&intent).into()],
            )
            .clear_ex()?;
            Ok(rx)
        })
        .inspect_err(|_| {
            let _ = MUTEX_PERM_REQ.lock().unwrap().take();
        })?;

        Ok(Some(Self { receiver }))
    }

    /// Blocks on waiting the permission request and returns the result.
    ///
    /// Warning: Blocking in the `android_main()` thread will block the future's completion if it
    /// depends on event processing in this thread (check your glue crate like `android_activity`).
    pub fn wait(self) -> RequestResult {
        #[cfg(not(feature = "futures"))]
        {
            self.receiver.recv().unwrap_or_default()
        }
        #[cfg(feature = "futures")]
        {
            futures_lite::future::block_on(self).unwrap_or_default()
        }
    }
}

#[cfg(feature = "futures")]
impl std::future::Future for PermissionRequest {
    type Output = Result<RequestResult, futures_channel::oneshot::Canceled>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use futures_lite::FutureExt;
        self.receiver.poll(cx)
    }
}

fn get_perm_activity_class() -> Result<&'static JObject<'static>, Error> {
    static PERM_ACTIVITY_CLASS: OnceLock<GlobalRef> = OnceLock::new();
    if PERM_ACTIVITY_CLASS.get().is_none() {
        jni_with_env(|env| {
            let class_loader = get_helper_class_loader()?;
            let class = class_loader.load_class("rust/jniminhelper/PermActivity")?;
            // register `perm_callback()`
            let native_method = NativeMethod {
                name: "nativeOnRequestPermissionsResult".into(),
                sig: "([Ljava/lang/String;[I)V".into(),
                fn_ptr: perm_callback as *mut _,
            };
            env.register_native_methods(class.as_class(), &[native_method])
                .map_err(jni_clear_ex)?;
            let _ = PERM_ACTIVITY_CLASS.set(class);
            Ok(())
        })?;
    }
    Ok(PERM_ACTIVITY_CLASS.get().unwrap())
}

extern "C" fn perm_callback<'a>(
    mut env: JNIEnv<'a>,
    _this: JObject<'a>,
    permissions: JObjectArray<'a>,
    grant_results: JIntArray<'a>,
) {
    let Some(sender) = MUTEX_PERM_REQ.lock().unwrap().take() else {
        warn!("Unexpected: perm_callback() received, but MUTEX_PERM_REQ is None.");
        return;
    };

    if permissions.is_null() || grant_results.is_null() {
        warn!("Unexpected: perm_callback() received null.");
        let _ = sender.send(Vec::new());
        return; // it should be impossible
    }

    let env = &mut env;

    let mut result = Vec::new();
    let Ok(permissions) = read_object_array(&permissions, env) else {
        warn!("Error in perm_callback(): read_object_array() failed.");
        return;
    };
    let mut grant_vals = vec![0; permissions.len()];
    if env
        .get_int_array_region(&grant_results, 0, &mut grant_vals[..])
        .is_err()
    {
        warn!("Error in perm_callback(): get_int_array_region() failed.");
        return;
    }
    for (i, perm) in permissions.iter().enumerate() {
        let Ok(perm) = perm.get_string(env) else {
            warn!("Error in perm_callback(): get_string() failed.");
            return;
        };
        result.push((perm, grant_vals[i] == PERMISSION_GRANTED));
    }

    if let Err(e) = sender.send(result) {
        warn!("Error in perm_callback(): sender.send() failed: {e:?}.");
    }
}
