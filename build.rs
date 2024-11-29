// Based on:
// <https://docs.rs/crate/i-slint-backend-android-activity/1.8.0/source/build.rs>
// <https://docs.rs/crate/robius-authentication/0.1.1/source/build.rs>

use std::{env, path::PathBuf, process::Command};

fn main() {
    let (javac, java_ver) = get_javac_path_ver();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=InvocHdl.java");
    let invoc_hdl_src_path =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("InvocHdl.java");

    if target_os == "android" {
        println!("cargo:rerun-if-changed=BroadcastRec.java");
        let broadcast_rec_src_path =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("BroadcastRec.java");

        let out_class_dir = out_dir.join("rust").join("jniminhelper");
        let out_class_paths = [
            out_class_dir.join("InvocHdl.class"),
            out_class_dir.join("BroadcastRec.class"),
            out_class_dir.join("BroadcastRec$BroadcastRecHdl.class"),
        ];

        let android_jar_path =
            android_build::android_jar(None).expect("Failed to find android.jar");
        let d8_jar_path = android_build::android_d8_jar(None).expect("Failed to find d8.jar");

        // Compile the .java file into a .class file.
        assert!(
            android_build::JavaBuild::new()
                .class_path(android_jar_path.clone())
                .classes_out_dir(out_dir.clone())
                .file(invoc_hdl_src_path)
                .file(broadcast_rec_src_path)
                .command()
                .expect("failed to generate javac command")
                .args(if java_ver != 8 {
                    &["--release", "8"]
                } else {
                    &[] as &[&str]
                })
                .status()
                .expect("failed to acquire exit status for javac invocation")
                .success(),
            "javac invocation failed"
        );
        assert!(
            android_build::JavaRun::new()
                .class_path(d8_jar_path)
                .main_class("com.android.tools.r8.D8")
                .arg("--classpath")
                .arg(android_jar_path)
                .arg("--output")
                .arg(out_dir.as_os_str())
                .arg("--min-api")
                .arg("20") // disable multidex
                .args(out_class_paths.iter())
                .run()
                .expect("failed to acquire exit status for java d8.jar invocation")
                .success(),
            "java d8.jar invocation failed"
        );
    } else {
        assert!(
            Command::new(javac)
                .arg("-d")
                .arg(out_dir)
                .arg(invoc_hdl_src_path)
                .status()
                .expect("failed to acquire exit status for javac invocation")
                .success(),
            "javac invocation failed"
        );
    }
}

fn get_javac_path_ver() -> (PathBuf, i32) {
    let javac =
        PathBuf::from(java_locator::locate_java_home().expect("Failed to locate java home"))
            .join("bin")
            .join("javac");

    let o = Command::new(&javac)
        .arg("-version")
        .output()
        .expect("javac invocation failed");
    if !o.status.success() {
        panic!(
            "Failed to get javac version: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let mut version_output = String::from_utf8_lossy(&o.stdout);
    if version_output.is_empty() {
        // old version of java used stderr
        version_output = String::from_utf8_lossy(&o.stderr);
    }
    let version = version_output.split_whitespace().nth(1).unwrap_or_default();
    let mut java_ver: i32 = version
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    if java_ver == 1 {
        // Before java 9, the version was something like javac 1.8
        java_ver = version
            .split('.')
            .nth(1)
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
    }
    if java_ver < 8 {
        panic!("The minimum required version is Java 8. Detected Java version: {java_ver}");
    }
    (javac, java_ver)
}
