// Based on:
// <https://docs.rs/crate/i-slint-backend-android-activity/1.8.0/source/build.rs>
// <https://docs.rs/crate/robius-authentication/0.1.1/source/build.rs>

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let javac_path_ver = get_javac_path_ver();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let src_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_class_dir = out_dir.join("rust").join("jniminhelper");

    println!("cargo:rerun-if-changed=InvocHdl.java");
    let invoc_hdl_src_path = src_dir.join("InvocHdl.java");

    if target_os == "android" {
        println!("cargo:rerun-if-changed=BroadcastRec.java");
        let broadcast_rec_src_path = src_dir.join("BroadcastRec.java");

        let out_class_paths = [
            out_class_dir.join("InvocHdl.class"),
            out_class_dir.join("BroadcastRec.class"),
            out_class_dir.join("BroadcastRec$BroadcastRecHdl.class"),
        ];

        let android_jar_path = android_build::android_jar(None);
        let d8_jar_path = android_build::android_d8_jar(None);

        let buildable = if javac_path_ver.is_none() {
            println!("cargo::warning=Failed to locate java home.");
            false
        } else if android_jar_path.is_none() {
            println!("cargo::warning=Failed to find android.jar.");
            false
        } else if d8_jar_path.is_none() {
            println!("cargo::warning=Failed to find d8.jar.");
            false
        } else {
            true
        };

        if buildable {
            let (java_ver, android_jar_path, d8_jar_path) = (
                javac_path_ver.unwrap().1,
                android_jar_path.unwrap(),
                d8_jar_path.unwrap(),
            );
            // Compiles .java files into .class files.
            assert!(
                android_build::JavaBuild::new()
                    .class_path(android_jar_path.clone())
                    .classes_out_dir(out_dir.clone())
                    .file(invoc_hdl_src_path)
                    .file(broadcast_rec_src_path)
                    .command()
                    .expect("Failed to generate javac command")
                    .args(if java_ver != 8 {
                        &["--release", "8"]
                    } else {
                        &[] as &[&str]
                    })
                    .status()
                    .expect("Failed to acquire exit status for javac invocation")
                    .success(),
                "javac invocation failed"
            );
            // Makes the dex file.
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
                    .expect("Failed to acquire exit status for java d8.jar invocation")
                    .success(),
                "java d8.jar invocation failed"
            );
        } else {
            println!("cargo::warning=Falling back to the unmanaged prebuilt dex.");
            let prebuilt_dex_path = src_dir.join("classes.dex");
            let out_dex_path = out_dir.join("classes.dex");
            fs::copy(prebuilt_dex_path, out_dex_path)
                .expect("Failed to access the prebuilt dex file");
        }
    } else {
        println!("Building for PC platform...");
        if let Some((javac, java_ver)) = javac_path_ver {
            assert!(
                Command::new(javac)
                    .arg("-d")
                    .arg(out_dir)
                    .arg(invoc_hdl_src_path)
                    .args(if java_ver != 8 {
                        &["--release", "8"]
                    } else {
                        &[] as &[&str]
                    })
                    .status()
                    .expect("Failed to acquire exit status for javac invocation")
                    .success(),
                "javac invocation failed"
            );
        } else {
            println!(
                "cargo::warning=Java home not found, falling back to the unmanaged prebuilt class."
            );

            if !out_class_dir.try_exists().unwrap() {
                fs::DirBuilder::new()
                    .recursive(true)
                    .create(&out_class_dir)
                    .expect("Failed to create output directories");
            }

            let prebuilt_class_path = src_dir.join("InvocHdl.class");
            let out_class_path = out_class_dir.join("InvocHdl.class");
            fs::copy(prebuilt_class_path, out_class_path)
                .expect("Failed to access the prebuilt class file");
        }
    }
}

fn get_javac_path_ver() -> Option<(PathBuf, i32)> {
    let javac = PathBuf::from(java_locator::locate_java_home().ok()?)
        .join("bin")
        .join("javac");

    let o = Command::new(&javac).arg("-version").output().ok()?;
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
    Some((javac, java_ver))
}
