# Changes

## 0.2.5
* Prints Rust stack trace for PC platforms and for JNI errors other than Java exceptions.
* Gets the Android context object from `ActivityThread.currentActivityThread().getApplication()` and prints a warning message in case of `ndk_context::android_context().context()` is null: functionalities related to the UI or `Activity` will not work, and the glue crate should be checked.

## 0.2.3
* Fixed the bug of panicking during building instead of using the prebuilt dex fallback when some environment variables required by `android-build` are missing.
* Removed `android-build` build dependency, because of its complicated configuration.

## 0.2.2
* Fixed the problem of possible fatal exception when calling `BroadcastReceiver::unregister()` for an unregistered receiver.
* Eliminated the `javac` warning for `InvocHdl.java`.

## 0.2.1
* Fixed a problem about the performance cache in the `convert` module.
* Added `is_same_object()`, `equals()` and `to_string()` in trait `JObjectGet`.
* Added `get_intent_action()` in `BroadcastReceiver` and `BroadcastWaiter`.
* Added `count_received()`, `take_next()` and `futures_core::Stream::size_hint()` implementation in `BroadcastWaiter`.

## 0.2.0
* Optimized the API.
* Fixed the problem of not being able to create proxies for custom interfaces on desktop platforms.
* Introduced the optional asynchronous `BroadcastWaiter`.

## 0.1.1
* Fixed doc.rs build problem by prebuilt class/dex file fallback.

## 0.1.0
* Initial release.
