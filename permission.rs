use std::sync::Mutex;

#[cfg(not(feature = "futures"))]
use std::sync::mpsc::{Receiver, Sender, channel};

#[cfg(feature = "futures")]
use futures_channel::oneshot::{Receiver, Sender, channel};

use crate::{
    android::{android_api_level, get_android_context, get_helper_class_loader},
    jni_with_env,
    receiver::Intent,
};

use jni::{
    Env,
    errors::Error,
    objects::{JClass, JIntArray, JObjectArray, JString},
    refs::Reference,
};

const PERMISSION_GRANTED: i32 = 0;
const EXTRA_PERM_ARRAY: &str = "rust.jniminhelper.perm_array";
const EXTRA_TITLE: &str = "rust.jniminhelper.perm_activity_title";

jni::bind_java_type! {
    PermActivity => "rust.jniminhelper.PermActivity",
    native_methods {
        fn native_on_request_permissions_result(permissions: JString[], grant_results: jint[]),
    },
}

type RequestResult = Vec<(String, bool)>;

static MUTEX_PERM_REQ: Mutex<Option<Sender<RequestResult>>> = Mutex::new(None);

/// Android runtime permission request utility.
///
/// Using this utility *requires* the activity `rust.jniminhelper.PermActivity` to be declared
/// in the `AndroidManifest.xml`, and this activity must be compiled in the package's `classes.dex`
/// file. `PermActivity.java` can be found in the source code.
///
/// For native activity applications, `cargo-apk` does not support these things at the time of
/// publishing this version of `jni-min-helper` (`cargo-apk2` has introduced these features).
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
            let context = get_android_context();
            let permission = JString::new(env, permission)?;
            context
                .check_self_permission(env, permission)
                .map(|i| i == PERMISSION_GRANTED)
        })
    }

    /// Returns true if there is an ongoing request managed by this crate.
    pub fn is_pending() -> bool {
        MUTEX_PERM_REQ.lock().unwrap().is_some()
    }

    /// Starts a permission request for permission names listed in `permissions`.
    /// Returns `Error::TryLock` if a previous request is unfinished;
    /// returns `Ok(None)` if all permissions are already granted or the Android
    /// API level is less than 23.
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
            let loader = jni::refs::LoaderContext::Loader(get_helper_class_loader()?);
            let _ = PermActivityAPI::get(env, &loader)?;
            let cls_perm = PermActivity::lookup_class(env, &loader)?;

            let context = get_android_context();
            let intent = Intent::new(env)?;
            use std::ops::Deref;
            intent.set_class(env, context, AsRef::<JClass>::as_ref(&cls_perm.deref()))?;

            let extra_title = JString::new(env, EXTRA_TITLE)?;
            let title = JString::new(env, title)?;
            intent.put_extra_string(env, extra_title, title)?;

            let arr_perms = JObjectArray::<JString>::new(env, perms.len(), JString::null())?;
            for (i, perm) in perms.iter().enumerate() {
                let perm = JString::new(env, perm)?;
                arr_perms.set_element(env, i, perm)?;
            }
            let extra_perm_array = JString::new(env, EXTRA_PERM_ARRAY)?;
            intent.put_extra_string_array(env, &extra_perm_array, &arr_perms)?;

            let (tx, rx) = channel();
            MUTEX_PERM_REQ.lock().unwrap().replace(tx);

            context.start_activity(env, &intent)?;
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

impl PermActivityNativeInterface for PermActivityAPI {
    type Error = Error;
    fn native_on_request_permissions_result<'local>(
        env: &mut Env<'local>,
        _this: PermActivity<'local>,
        permissions: JObjectArray<'local, jni::objects::JString<'local>>,
        grant_results: JIntArray<'local>,
    ) -> ::std::result::Result<(), Self::Error> {
        let Some(sender) = MUTEX_PERM_REQ.lock().unwrap().take() else {
            warn!("Unexpected: perm_callback() received, but MUTEX_PERM_REQ is None.");
            return Ok(());
        };

        if permissions.is_null() || grant_results.is_null() {
            // it should be unreachable
            warn!("Unexpected: perm_callback() received null.");
            let _ = sender.send(Vec::new());
            return Err(Error::NullPtr("Unexpected: perm_callback() received null."));
        }

        let mut result = Vec::new();

        let mut grant_vals = vec![0; grant_results.len(env)?];
        grant_results.get_region(env, 0, &mut grant_vals)?;
        for (i, &res_val) in grant_vals.iter().enumerate() {
            result.push((
                permissions.get_element(env, i)?.to_string(),
                res_val == PERMISSION_GRANTED,
            ));
        }

        if let Err(e) = sender.send(result) {
            warn!("Error in perm_callback(): sender.send() failed: {e:?}.");
        }
        Ok(())
    }
}
