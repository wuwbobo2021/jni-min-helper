# Changes

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
