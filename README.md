# jni-min-helper
Minimal helper for `jni-rs`, supporting dynamic proxies, Android dex embedding, runtime permission request and broadcast receiver. Used for calling Java code from Rust.

WORK IN PROGRESS: This crate is being ported to `jni` 0.22. Currently the dynamic proxy cannot even work on PC (Linux/Windows, OpenJDK/Oracle Java) platforms. Android-specific code haven't been adapted yet. Do not make use of any code under the main branch for any realistic purpose.
