use crate::{JObjectAutoLocal, convert::*, jni_clear_ex, jni_with_env, loader::*, proxy::*};

use jni::{
    JNIEnv,
    errors::Error,
    objects::{GlobalRef, JMethodID, JObject},
    signature::ReturnType,
};

/// Handles `android.content.BroadcastReceiver` object backed by `JniProxy`.
///
/// Register/unregister functions are provided for convenience, but not for
/// maintaining any internal state. However, `unregister()` is called on `drop()`.
#[derive(Debug)]
pub struct BroadcastReceiver {
    receiver: GlobalRef,
    proxy: Option<JniProxy>, // taken on `forget()`
    forget: bool,
}

impl AsRef<JObject<'static>> for BroadcastReceiver {
    fn as_ref(&self) -> &JObject<'static> {
        self.receiver.as_obj()
    }
}

impl std::ops::Deref for BroadcastReceiver {
    type Target = JObject<'static>;
    fn deref(&self) -> &Self::Target {
        self.receiver.as_obj()
    }
}

impl Drop for BroadcastReceiver {
    fn drop(&mut self) {
        if !self.forget {
            let _ = jni_with_env(|env| {
                self.unregister_inner(env)
                    .map_err(crate::jni_clear_ex_ignore)
            });
        }
    }
}

impl BroadcastReceiver {
    /// Creates a `android.content.BroadcastReceiver` object backed by the Rust closure.
    ///
    /// The two Java object references passed to the closure are `context` and `intent`.
    ///
    /// Note: It makes sure that no exception can be thrown from `onReceive()`.
    pub fn build(
        handler: impl for<'a> Fn(&mut JNIEnv<'a>, &JObject<'a>, &JObject<'a>) -> Result<(), Error>
        + Send
        + Sync
        + 'static,
    ) -> Result<Self, Error> {
        jni_with_env(|env| {
            let loader = get_helper_class_loader()?;
            let cls_rec = loader.load_class("rust/jniminhelper/BroadcastRec")?;
            let cls_rec_hdl =
                loader.load_class("rust/jniminhelper/BroadcastRec$BroadcastRecHdl")?;

            let proxy = JniProxy::build(
                env,
                Some(loader),
                [cls_rec_hdl.as_class()],
                move |env, method, args| {
                    if method.get_method_name(env)? == "onReceive" && args.len() == 2 {
                        // `jni_clear_ex` may be called inside the closure on exception;
                        // if not, then this will prevent the exception from throwing.
                        let _ = handler(env, args[0], args[1]).map_err(crate::jni_clear_ex_silent);
                        let _ = env.exception_clear();
                    }
                    JniProxy::void(env)
                },
            )?;

            let receiver = env
                .new_object(
                    cls_rec.as_class(),
                    "(Lrust/jniminhelper/BroadcastRec$BroadcastRecHdl;)V",
                    &[(&proxy).into()],
                )
                .global_ref(env)?;

            Ok(Self {
                receiver,
                proxy: Some(proxy),
                forget: false,
            })
        })
    }

    /// Registers the receiver to the current Android context.
    pub fn register(&self, intent_filter: &JObject<'_>) -> Result<(), Error> {
        jni_with_env(|env| {
            let context = android_context();
            env.call_method(
                context,
                "registerReceiver",
                "(Landroid/content/BroadcastReceiver;Landroid/content/IntentFilter;)Landroid/content/Intent;",
                &[(&self.receiver).into(), (&intent_filter).into()]
            )
            .clear_ex()
        })
    }

    /// Registers the receiver to the current Android context, with an intent filter
    /// that matches a single `action` with no data.
    pub fn register_for_action(&self, action: &str) -> Result<(), Error> {
        jni_with_env(|env| {
            let action = action.new_jobject(env)?;
            let filter = env
                .new_object(
                    "android/content/IntentFilter",
                    "(Ljava/lang/String;)V",
                    &[(&action).into()],
                )
                .auto_local(env)?;
            self.register(&filter)
        })
    }

    /// Unregister the previously registered broadcast receiver. All filters that have been
    /// registered for this receiver will be removed.
    #[inline(always)]
    pub fn unregister(&self) -> Result<(), Error> {
        jni_with_env(|env| self.unregister_inner(env))
    }

    fn unregister_inner(&self, env: &mut JNIEnv<'_>) -> Result<(), Error> {
        let context = android_context();
        env.call_method(
            context,
            "unregisterReceiver",
            "(Landroid/content/BroadcastReceiver;)V",
            &[(&self.receiver).into()],
        )
        .map(|_| ())
    }

    /// Gets the action name of the received `android.content.Intent`.
    #[inline]
    pub fn get_intent_action<'a>(
        intent: impl AsRef<JObject<'a>>,
        env: &mut JNIEnv<'_>,
    ) -> Result<String, Error> {
        use std::sync::OnceLock;
        static STORE: OnceLock<(GlobalRef, JMethodID)> = OnceLock::new();
        if STORE.get().is_none() {
            let class_intent = env.find_class("android/content/Intent").global_ref(env)?;
            let method_get_action = env
                .get_method_id(&class_intent, "getAction", "()Ljava/lang/String;")
                .map_err(jni_clear_ex)?;
            let _ = STORE.set((class_intent, method_get_action));
        }
        let store = STORE.get().unwrap();
        let (class, method) = (store.0.as_class(), &store.1);

        intent.class_check(class, env)?;
        unsafe { env.call_method_unchecked(intent, method, ReturnType::Object, &[]) }
            .get_object(env)?
            .get_string(env)
    }

    /// Leaks the Rust handler and returns the global reference of the broadcast
    /// receiver. It prevents deregistering of the receiver on dropping. This is
    /// useful if it is created for *once* in the program.
    pub fn forget(mut self) -> GlobalRef {
        self.forget = true;
        self.proxy.take().unwrap().forget();
        self.receiver.clone()
    }
}

#[cfg(feature = "futures")]
pub use waiter::*;

#[cfg(feature = "futures")]
mod waiter {
    use super::*;
    use futures_lite::StreamExt;
    use std::{
        collections::VecDeque,
        pin::Pin,
        sync::{Arc, Mutex},
        task,
        time::Duration,
    };

    /// Waits for intents received by the managed `BroadcastReceiver`.
    #[derive(Debug)]
    pub struct BroadcastWaiter {
        receiver: BroadcastReceiver,
        inner: Arc<BroadcastWaiterInner>,
    }

    #[derive(Debug)]
    struct BroadcastWaiterInner {
        waker: atomic_waker::AtomicWaker,
        intents: Mutex<VecDeque<GlobalRef>>,
    }

    impl BroadcastWaiter {
        /// Creates the waiter with a new broadcast receiver.
        /// `actions` are passed to `BroadcastReceiver::register_for_action()`.
        pub fn build(
            actions: impl IntoIterator<Item = impl AsRef<str>>,
        ) -> Result<Self, jni::errors::Error> {
            let inner = Arc::new(BroadcastWaiterInner {
                waker: atomic_waker::AtomicWaker::new(),
                intents: Mutex::new(VecDeque::new()),
            });
            let inner_weak = Arc::downgrade(&inner);
            let receiver = BroadcastReceiver::build(move |env, _, intent| {
                if intent.is_null() {
                    return Ok(());
                }
                let Some(inner) = inner_weak.upgrade() else {
                    return Ok(());
                };
                let intent = env.new_global_ref(intent).map_err(jni_clear_ex)?;
                inner.intents.lock().unwrap().push_back(intent);
                inner.waker.wake();
                Ok(())
            })?;
            for action in actions {
                receiver.register_for_action(action.as_ref())?;
            }
            Ok(Self { receiver, inner })
        }

        /// Exposes a reference to the broadcast receiver for manual registration.
        pub fn receiver(&self) -> &BroadcastReceiver {
            &self.receiver
        }

        /// Returns the amount of received intents available for checking.
        pub fn count_received(&self) -> usize {
            self.inner.intents.lock().unwrap().len()
        }

        /// Takes the next received intent if available. This shouldn't conflict
        /// with the asynchonous feature (which requires a mutable reference).
        pub fn take_next(&self) -> Option<GlobalRef> {
            self.inner.intents.lock().unwrap().pop_front()
        }

        /// Waits for receiving an intent.
        /// Note: Waiting in the `android_main()` thread will prevent it from receiving.
        pub fn wait_timeout(&mut self, timeout: Duration) -> Option<GlobalRef> {
            let fut = BroadcastWaiterFuture { waiter: self };
            block_for_timeout(fut, timeout).unwrap_or(None)
        }

        /// Gets the action name of the received `android.content.Intent`.
        #[inline(always)]
        pub fn get_intent_action<'a>(
            intent: impl AsRef<JObject<'a>>,
            env: &mut JNIEnv<'_>,
        ) -> Result<String, Error> {
            BroadcastReceiver::get_intent_action(intent, env)
        }
    }

    /// Convenient blocker for asynchronous functions, based on `futures_lite` and `futures_timer`.
    /// Warning: Blocking in the `android_main()` thread will block the future's completion if it
    /// depends on event processing in this thread (check your glue crate like `android_activity`).
    pub fn block_for_timeout<T>(
        fut: impl std::future::Future<Output = T>,
        dur: std::time::Duration,
    ) -> Option<T> {
        use futures_lite::{FutureExt, future::block_on};
        let fut_comp = async { Some(fut.await) };
        let fut_cancel = async {
            futures_timer::Delay::new(dur).await;
            None
        };
        block_on(fut_comp.or(fut_cancel))
    }

    impl futures_core::Stream for BroadcastWaiter {
        type Item = GlobalRef;

        fn poll_next(
            self: Pin<&mut Self>,
            cx: &mut task::Context<'_>,
        ) -> task::Poll<Option<Self::Item>> {
            // <https://docs.rs/atomic-waker/1.1.2/atomic_waker/struct.AtomicWaker.html#examples>
            if let Some(intent) = self.take_next() {
                return task::Poll::Ready(Some(intent));
            }
            self.inner.waker.register(cx.waker());
            if let Some(intent) = self.take_next() {
                task::Poll::Ready(Some(intent))
            } else {
                task::Poll::Pending
            }
        }

        // Explanation for this trait function: the actual remaining length should fall
        // in the returned estimation "range" (min, max).
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.count_received(), None)
        }
    }

    struct BroadcastWaiterFuture<'a> {
        waiter: &'a mut BroadcastWaiter,
    }

    impl<'a> std::future::Future for BroadcastWaiterFuture<'a> {
        type Output = Option<GlobalRef>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
            if let task::Poll::Ready(intent) = self.waiter.poll_next(cx) {
                task::Poll::Ready(intent)
            } else {
                task::Poll::Pending
            }
        }
    }
}
