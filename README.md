# jni-min-helper
Minimal helper for `jni-rs`, supporting dynamic proxies, Android dex embedding and broadcast receiver. Used for calling Java code from Rust.

While the JNI interface was initially designed for calling native code from Java, this library provides a bit more convenient functions for handling exceptions and avoiding memory leaks, preventing the Rust application from crashing.

This crate aims to be a reliable dependency for cross-platform libraries to introduce initial Android support, without relying on some specific version of Gradle. Older Android versions can be supported as well.

Documentation: <https://docs.rs/jni-min-helper/latest>.

The dynamic proxy implementation is inspired by [droid-wrap-utils](https://crates.io/crates/droid-wrap-utils). `droid-wrap` is another project with greater ambition, however the initial version is less reliable.

Check the source of this crate to see how a dex file can be embedded. Note: `InvocHdl.class` and `classes.dex` are *unmanaged* prebuilt files for docs.rs to build documentation successfully. `build.rs` will print a warning and use the prebuilt file as a fallback on failure. 

## Desktop

To test it on a desktop OS, just make sure the JDK is installed, then add `jni-min-helper` dependency into your new binary crate, fill in `main()` with the example given in `jni_min_helper::JniProxy` documentation.

Of course, the dex class loader and the broadcast receiver are not available. Call `jni_set_vm()` (before using other functions) to prevent the library from creating a new JVM by itself.

## Android

Configure environment variables required by [android-build](https://docs.rs/android-build/latest/android_build); make sure the Rust target `aarch64-linux-android` and [cargo-apk](https://docs.rs/crate/cargo-apk/latest) are installed.

### Registering a broadcast receiver

```toml
[package]
name = "android-simple-test"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
log = "0.4"
jni-min-helper = "0.2"
android-activity = { version = "0.6", features = ["native-activity"] }
android_logger = "0.14"

[lib]
name = "android_simple_test"
crate-type = ["cdylib"]
path = "main.rs"

[package.metadata.android]
package = "com.example.android_simple_test"
build_targets = [ "aarch64-linux-android" ]

[package.metadata.android.sdk]
min_sdk_version = 16
target_sdk_version = 30

[[package.metadata.android.uses_permission]]
name = "android.permission.ACCESS_NETWORK_STATE"
```

```rust
use android_activity::{AndroidApp, MainEvent, PollEvent};
use jni::{errors::Error, objects::JObject, JNIEnv};
use jni_min_helper::*;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag(android_app_name().as_bytes()),
    );

    let receiver = BroadcastReceiver::build(on_receive).unwrap();
    receiver
        .register_for_action("android.net.conn.CONNECTIVITY_CHANGE")
        .unwrap();

    let mut on_destroy = false;
    loop {
        app.poll_events(None, |event| match event {
            PollEvent::Main(MainEvent::Destroy) => {
                on_destroy = true;
            }
            _ => (),
        });
        if on_destroy {
            return;
        }
    }
}

fn on_receive<'a>(
    env: &mut JNIEnv<'a>,
    context: &JObject<'a>,
    intent: &JObject<'a>,
) -> Result<(), Error> {
    let action = env
        .call_method(intent, "getAction", "()Ljava/lang/String;", &[])
        .get_object(env)?
        .get_string(env)?;
    log::info!("Received an intent of action '{action}'.");

    let connectivity_service = "connectivity".new_jobject(env)?;

    let conn_man = env
        .call_method(
            context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[(&connectivity_service).into()],
        )
        .get_object(env)?;

    let net_info = env
        .call_method(
            &conn_man,
            "getActiveNetworkInfo",
            "()Landroid/net/NetworkInfo;",
            &[],
        )
        .get_object(env)?;

    let connected = if !net_info.is_null() {
        env.call_method(&net_info, "isConnected", "()Z", &[])
            .get_boolean()?
    } else {
        false
    };

    let msg = if connected {
        "Network is connected."
    } else {
        "Network is currently disconnected."
    };
    log::info!("{msg}");

    let msg = msg.new_jobject(env)?;
    let toast = env
        .call_static_method(
            "android/widget/Toast",
            "makeText",
            "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;",
            &[context.into(), (&msg).into(), 0.into()],
        )
        .get_object(env)?;
    env.call_method(&toast, "show", "()V", &[]).clear_ex()
}
```

Or use the `futures-lite` blocker of the asynchronous broadcast waiter:

```toml
jni-min-helper = { version = "0.2", features = ["futures"] }
```

```rust
use android_activity::{AndroidApp, MainEvent, PollEvent};
use jni_min_helper::*;
use std::time::Duration;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag(android_app_name().as_bytes()),
    );

    std::thread::spawn(background_loop);

    let mut on_destroy = false;
    loop {
        app.poll_events(None, |event| match event {
            PollEvent::Main(MainEvent::Destroy) => {
                on_destroy = true;
            }
            _ => (),
        });
        if on_destroy {
            return;
        }
    }
}

fn background_loop() {
    let mut waiter = BroadcastWaiter::build([
        "android.intent.action.TIME_TICK",
        "android.net.conn.CONNECTIVITY_CHANGE",
    ])
    .unwrap();
    log::info!("Built broadcast waiter.");
    loop {
        if let Some(intent) = waiter.wait_timeout(Duration::from_secs(1)) {
            let env = &mut jni_attach_vm().unwrap();
            let action = env
                .call_method(intent, "getAction", "()Ljava/lang/String;", &[])
                .get_object(env)
                .unwrap()
                .get_string(env)
                .unwrap();
            log::info!("Received an intent of action '{action}'.");
        }
    }
}
```

Build it with `cargo-apk` and install it on the Android device, then check the log output: `adb logcat android_simple_test:D '*:S'`.

Note: building for the release profile produces a much smaller package.

### Receiving result from the chooser dialog

```rust
use android_activity::{AndroidApp, MainEvent, PollEvent};
use jni_min_helper::*;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag(android_app_name().as_bytes()),
    );

    log::info!("starting dialog_test 1...");
    if dialog_test(Some(&app)) {
        log::info!("starting dialog_test 2...");
        // this will not dismiss the dialog on main stop event.
        std::thread::spawn(|| dialog_test(None));
    }

    let mut on_destroy = false;
    loop {
        app.poll_events(None, |event| match event {
            PollEvent::Main(MainEvent::Destroy) => {
                on_destroy = true;
            }
            _ => (),
        });
        if on_destroy {
            return;
        }
    }
}

fn dialog_test(app: Option<&AndroidApp>) -> bool {
    let result = chooser_dialog(app, "Choose", &["i", "j", "k"]).unwrap();
    if let Some(c) = result {
        log::info!("The user choosed {c}.");
        true
    } else {
        log::info!("The dialog has been dismissed.");
        false
    }
}

// Provide the `app` reference if it is being called in the native main thread;
// Otherwise, `app` should be `None` to make it work.
fn chooser_dialog<'a>(
    app: Option<&AndroidApp>,
    title: &str,
    choices: &'a [&'a str],
) -> Result<Option<&'a str>, jni::errors::Error> {
    use jni::{
        objects::{JObject, JObjectArray},
        sys::jsize,
    };
    use std::sync::{mpsc, Arc, Mutex};
    let env = &mut jni_attach_vm()?;
    let context = android_context();

    // creates the dialog builder
    let dialog_builder = env
        .new_object(
            "android/app/AlertDialog$Builder",
            "(Landroid/content/Context;)V",
            &[(&context).into()],
        )
        .auto_local(env)?;

    let title = title.new_jobject(env)?;

    // converts choice items to Java array
    let choice_items = env
        .new_object_array(choices.len() as jsize, "java/lang/String", JObject::null())
        .auto_local(env)?;
    let choice_items: &JObjectArray<'_> = choice_items.as_ref().into();
    for (i, choice_name) in choices.iter().enumerate() {
        let choice_name = choice_name.new_jobject(env)?;
        env.set_object_array_element(choice_items, i as jsize, &choice_name)
            .unwrap();
    }

    let (tx1, rx) = mpsc::channel();
    let tx2 = tx1.clone();

    // creates OnClickListener
    let on_click_listener = JniProxy::build(
        None,
        ["android/content/DialogInterface$OnClickListener"],
        move |env, method, args| {
            if method.get_method_name(env)? == "onClick" {
                let _ = tx1.send(Some(args[1].get_int(env)?));
            }
            JniProxy::void(env)
        },
    )
    .unwrap();

    // creates OnDismissListener
    let on_dismiss_listener = JniProxy::build(
        None,
        ["android/content/DialogInterface$OnDismissListener"],
        move |env, method, _| {
            if method.get_method_name(env)? == "onDismiss" {
                let _ = tx2.send(None);
            }
            JniProxy::void(env)
        },
    )
    .unwrap();

    // configure the dialog builder
    env.call_method(
        &dialog_builder,
        "setItems",
        "([Ljava/lang/CharSequence;Landroid/content/DialogInterface$OnClickListener;)Landroid/app/AlertDialog$Builder;",
        &[(&choice_items).into(), (&on_click_listener).into()]
    ).clear_ex()?;

    env.call_method(
        &dialog_builder,
        "setOnDismissListener",
        "(Landroid/content/DialogInterface$OnDismissListener;)Landroid/app/AlertDialog$Builder;",
        &[(&on_dismiss_listener).into()],
    )
    .clear_ex()?;

    env.call_method(
        &dialog_builder,
        "setTitle",
        "(Ljava/lang/CharSequence;)Landroid/app/AlertDialog$Builder;",
        &[(&title).into()],
    )
    .clear_ex()?;

    // creating and showing the dialog must be done in the Java main thread
    let dialog_builder = Ok(dialog_builder).globalize(env)?;
    let dialog_arc = Arc::new(Mutex::new(None));
    let dialog_arc_2 = dialog_arc.clone(); // Note: a weak reference might be used
    let _posted_runnable = post_on_java_main_thread(move |env| {
        let dialog = env
            .call_method(
                &dialog_builder,
                "create",
                "()Landroid/app/AlertDialog;",
                &[],
            )
            .get_object(env)
            .globalize(env)?;
        env.call_method(&dialog, "show", "()V", &[]).clear_ex()?;
        dialog_arc_2.lock().unwrap().replace(dialog);
        Ok(())
    })?;

    if let Some(r) = wait_recv(&rx, app) {
        Ok(r.map(|i| choices[i as usize]))
    } else {
        let dialog = dialog_arc.lock().unwrap().take();
        if let Some(dialog) = dialog {
            env.call_method(&dialog, "dismiss", "()V", &[]).clear_ex()?;
        }
        Ok(None)
    }
}

fn post_on_java_main_thread(
    runnable: impl Fn(&mut jni::JNIEnv) -> Result<(), jni::errors::Error> + Send + Sync + 'static,
) -> Result<Option<JniProxy>, jni::errors::Error> {
    let env = &mut jni_attach_vm()?;

    let runnable = JniProxy::build(None, ["java/lang/Runnable"], move |env, method, _| {
        if method.get_method_name(env)? == "run" {
            let _ = runnable(env);
            let _ = env.exception_clear();
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
        .get_object(env)?;
    let handler = env
        .new_object(
            "android/os/Handler",
            "(Landroid/os/Looper;)V",
            &[(&main_looper).into()],
        )
        .auto_local(env)?;
    let suc = env
        .call_method(
            &handler,
            "post",
            "(Ljava/lang/Runnable;)Z",
            &[(&runnable).into()],
        )
        .get_boolean()?;
    Ok(suc.then_some(runnable))
}

fn wait_recv<T>(rx: &std::sync::mpsc::Receiver<T>, app: Option<&AndroidApp>) -> Option<T> {
    if let Some(app) = app {
        // it runs in the native main thread
        let mut on_stop = false;
        loop {
            // `rx.recv()` may block forever.
            if let Ok(r) = rx.try_recv() {
                return Some(r);
            } else {
                // Let the native main thread process events from the Java main thread.
                // It's tested that `ndk::looper::ThreadLooper::poll_once()` doesn't work here,
                // check `android_activity::AndroidApp::poll_events()` documentation.
                app.poll_events(None, |event| {
                    if let PollEvent::Main(MainEvent::Stop) = event {
                        on_stop = true;
                    }
                });
                if on_stop {
                    return None;
                }
            }
        }
    } else {
        // it runs in another background thread
        rx.recv().ok()
    }
}
```

Note: this is definitely not a perfect implementation.
