use crate::{convert::*, jni_attach_vm, jni_clear_ex, loader::*, proxy::*, JObjectAutoLocal};

use jni::{
    errors::Error,
    objects::{GlobalRef, JObject},
    JNIEnv,
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
            let _ = self.unregister_inner().map_err(crate::jni_clear_ex_ignore);
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
        let env = &mut jni_attach_vm()?;

        let loader = get_helper_class_loader()?;
        let cls_rec = loader.load_class("rust/jniminhelper/BroadcastRec")?;
        let cls_rec_hdl = loader.load_class("rust/jniminhelper/BroadcastRec$BroadcastRecHdl")?;

        let proxy = JniProxy::build(
            Some(loader),
            [cls_rec_hdl.as_class()],
            move |env, method, args| {
                if method.get_method_name(env)? == "onReceive" && args.len() == 2 {
                    // usually, `jni_clear_ex` will be called inside the closure on exception;
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
    }

    /// Registers the receiver to the current Android context.
    pub fn register(&self, intent_filter: &JObject<'_>) -> Result<(), Error> {
        let env = &mut jni_attach_vm()?;
        let context = android_context();
        env.call_method(
            context,
            "registerReceiver",
            "(Landroid/content/BroadcastReceiver;Landroid/content/IntentFilter;)Landroid/content/Intent;",
            &[(&self.receiver).into(), (&intent_filter).into()]
        )
        .clear_ex()
    }

    /// Registers the receiver to the current Android context, with an intent filter
    /// that matches a single `action` with no data.
    pub fn register_for_action(&self, action: &str) -> Result<(), Error> {
        let env = &mut jni_attach_vm()?;
        let action = action.new_jobject(env)?;
        let filter = env
            .new_object(
                "android/content/IntentFilter",
                "(Ljava/lang/String;)V",
                &[(&action).into()],
            )
            .auto_local(env)?;
        self.register(&filter)
    }

    /// Unregister the previously registered broadcast receiver. All filters that have been
    /// registered for this receiver will be removed.
    #[inline(always)]
    pub fn unregister(&self) -> Result<(), Error> {
        self.unregister_inner().map_err(jni_clear_ex)
    }

    fn unregister_inner(&self) -> Result<(), Error> {
        let env = &mut jni_attach_vm()?;
        let context = android_context();
        env.call_method(
            context,
            "unregisterReceiver",
            "(Landroid/content/BroadcastReceiver;)V",
            &[(&self.receiver).into()],
        )
        .map(|_| ())
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

        /// Waits for receiving an intent. It does not work in the `android_main()` thread.
        pub fn wait_timeout(&mut self, timeout: Duration) -> Option<GlobalRef> {
            let fut = BroadcastWaiterFuture { waiter: self };
            block_for_timeout(fut, timeout).unwrap_or(None)
        }
    }

    /// Convenient blocker for asynchronous functions, based on `futures_lite`
    /// and `futures_timer`. It does not work in the `android_main()` thread.
    pub fn block_for_timeout<T>(
        fut: impl std::future::Future<Output = T>,
        dur: std::time::Duration,
    ) -> Option<T> {
        use futures_lite::{future::block_on, FutureExt};
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
            self.inner.waker.register(cx.waker());
            if let Some(intent) = self.inner.intents.lock().unwrap().pop_front() {
                task::Poll::Ready(Some(intent))
            } else {
                task::Poll::Pending
            }
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
