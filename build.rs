// Inspired by the build script of crate `i-slint-backend-android-activity`.
// For the Android target, new source files and even .jar dependencies can be added easily.
// Note: Newer JDK versions (including JDK 21 and above) may not work with Android D8
// if there are anonymous classes in the Java code, which produces files like `Cls$1.class`
// (fixed in build tools 35.0.0 ?). Currently `jni-min-helper` doesn't use anonymous classes.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    if env::var("CARGO_FEATURE_NO_PROXY").is_ok() {
        return;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let src_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("java");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // TODO: fix the possible unicode output problem.
    // Safety: this sets the variable for the current single-thread process
    // of the executable compiled from this build script.
    unsafe {
        env::set_var("JAVA_TOOL_OPTIONS", "-Duser.language=en");
    }

    if target_os == "android" {
        let sources = collect_files_with_ext(&src_dir, "java").unwrap();
        let android_jar = get_android_jar_path();
        let mut err_string = None;
        if android_jar.is_none() {
            err_string.replace("Failed to find android.jar.".to_string());
        } else if let Err(s) = compile_java_source(sources, [android_jar.unwrap()], out_dir.clone())
        {
            err_string.replace(s);
        } else if let Err(s) = build_dex_file(out_dir.clone(), [], out_dir.clone()) {
            // TODO: clean up OUT_DIR before `build_dex_file` in case of some classes were removed.
            err_string.replace(s);
        };
        if let Some(s) = err_string {
            for line in s.lines() {
                println!("cargo::warning={line}");
            }
            println!("cargo::warning=Falling back to the unmanaged prebuilt dex.");
            let prebuilt_dex_path = src_dir.join("classes.dex");
            let out_dex_path = out_dir.join("classes.dex");
            fs::copy(prebuilt_dex_path, out_dex_path)
                .expect("Failed to access the prebuilt dex file");
        }
    } else {
        println!("Building for PC platform...");
        if let Err(s) = compile_java_source([src_dir.join("InvocHdl.java")], [], out_dir.clone()) {
            for line in s.lines() {
                println!("cargo::warning={line}");
            }
            println!("cargo::warning=Falling back to the unmanaged prebuilt class.");
            let out_class_file_dir = out_dir.join("rust").join("jniminhelper");
            if !out_class_file_dir.try_exists().unwrap() {
                fs::DirBuilder::new()
                    .recursive(true)
                    .create(&out_class_file_dir)
                    .expect("Failed to create output directories");
            }
            let prebuilt_class_path = src_dir.join("InvocHdl.class");
            let out_class_path = out_class_file_dir.join("InvocHdl.class");
            fs::copy(prebuilt_class_path, out_class_path)
                .expect("Failed to access the prebuilt class file");
        }
    }
}

fn compile_java_source(
    source_paths: impl IntoIterator<Item = PathBuf>,
    class_paths: impl IntoIterator<Item = PathBuf>,
    output_dir: PathBuf,
) -> Result<(), String> {
    let (java_home, java_ver) = get_java_home_ver()?;
    if java_ver < 8 {
        return Err(format!(
            "The minimum required Java version is Java 8, detected version: {java_ver}."
        ));
    }

    let mut cmd = Command::new(java_home.join("bin").join("javac"));
    for java_src in source_paths {
        println!("cargo:rerun-if-changed={}", java_src.to_string_lossy());
        cmd.arg(java_src);
    }

    let mut classpath_param = std::ffi::OsString::new();
    let seperator = if std::path::MAIN_SEPARATOR == '\\' {
        ";"
    } else {
        ":"
    };
    for class_path in class_paths {
        classpath_param.push(class_path.as_os_str());
        classpath_param.push(seperator);
    }
    let mut classpath_param = classpath_param.into_string().unwrap();
    let _ = classpath_param.pop(); // remove the last seperator
    cmd.arg("-classpath").arg(classpath_param);

    cmd.arg("-d").arg(output_dir);
    if java_ver > 8 {
        cmd.arg("--release").arg("8");
    }
    cmd.arg("-encoding").arg("UTF-8");

    // Execute the command
    let result = cmd
        .output()
        .map_err(|e| format!("Failed to execute javac: {:?}", e))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Java compilation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
    }
}

fn build_dex_file(
    compiled_classes_path: PathBuf,
    jar_dependencies: impl IntoIterator<Item = PathBuf>,
    output_dir: PathBuf,
) -> Result<(), String> {
    let java = get_java_home_ver()?.0.join("bin").join("java");
    let d8_jar_path = get_d8_jar_path().ok_or("Failed to find d8.jar.".to_string())?;
    let android_jar_path =
        get_android_jar_path().ok_or("Failed to find android.jar.".to_string())?;

    let compiled_classes = collect_files_with_ext(&compiled_classes_path, "class")
        .map_err(|e| format!("Failed to walk through the compiled classes path: {e}."))?;
    let dependencies: Vec<_> = jar_dependencies.into_iter().collect();

    let mut cmd = Command::new(java);
    cmd.arg("-classpath")
        .arg(d8_jar_path)
        .arg("com.android.tools.r8.D8");
    cmd.arg("--lib").arg(android_jar_path);
    for dependency in dependencies.iter() {
        cmd.arg("--classpath").arg(dependency);
    }
    cmd.arg("--classpath").arg(compiled_classes_path);
    cmd.arg("--output").arg(output_dir);
    // disable multidex (workaround for the DexClassLoader before Android 8.0)
    cmd.arg("--min-api").arg("20");
    cmd.args(compiled_classes).args(dependencies.iter());

    // Execute the command
    let result = cmd
        .output()
        .map_err(|e| format!("Failed to execute d8.jar: {:?}", e))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "java d8.jar invocation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ))
    }
}

fn get_java_home_ver() -> Result<(PathBuf, i32), String> {
    println!("cargo:rerun-if-env-changed=JAVA_HOME");
    let java_home = java_locator::locate_java_home()
        .map(PathBuf::from)
        .map_err(|_| "Failed to locate java home.".to_string())?;

    let javac = java_home.join("bin").join("javac");
    let output = Command::new(&javac)
        .arg("-version")
        .output()
        .map_err(|e| format!("Failed to execute javac -version: {:?}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to get javac version: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut version_output = String::from_utf8_lossy(&output.stdout);
    if version_output.is_empty() {
        // old versions of java use stderr
        version_output = String::from_utf8_lossy(&output.stderr);
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
    if java_ver > 0 {
        Ok((java_home, java_ver))
    } else {
        Err(format!("Failed to parse javac version: '{version}'"))
    }
}

fn get_android_home() -> Option<PathBuf> {
    env_var("ANDROID_HOME")
        .or_else(|_| env_var("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .ok()
}

fn get_android_jar_path() -> Option<PathBuf> {
    let platforms_path = get_android_home()?.join("platforms");
    find_latest_version(&platforms_path, "android.jar")
        .map(|ver| platforms_path.join(ver).join("android.jar"))
}

fn get_d8_jar_path() -> Option<PathBuf> {
    let build_tools_path = get_android_home()?.join("build-tools");
    let d8_sub_path = Path::new("lib").join("d8.jar");
    find_latest_version(&build_tools_path, &d8_sub_path)
        .map(|ver| build_tools_path.join(ver).join(d8_sub_path))
}

/// Rerun the build script if the variable is changed. Do not use it for variables set by Cargo.
fn env_var(var: &str) -> Result<String, env::VarError> {
    println!("cargo:rerun-if-env-changed={}", var);
    env::var(var)
}

/// Finds subdirectories in which the subpath `arg` exists, and returns the maximum
/// item name in lexicographical order based on `Ord` impl of `std::path::Path`.
/// NOTE: the behavior can be changed in the future.
fn find_latest_version(base: impl AsRef<Path>, arg: impl AsRef<Path>) -> Option<String> {
    std::fs::read_dir(base)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join(arg.as_ref()).exists())
        .map(|entry| entry.file_name())
        .max()
        .and_then(|name| name.to_os_string().into_string().ok())
}

/// Collect all files with the given extension in the given directory recursively.
fn collect_files_with_ext(
    path: impl AsRef<Path>,
    extension: &str,
) -> std::io::Result<Vec<PathBuf>> {
    // From `std::fs::read_dir` examples: walking a directory only visiting files.
    fn visit_dirs(
        dir: impl AsRef<Path>,
        cb: &mut impl FnMut(&fs::DirEntry),
    ) -> std::io::Result<()> {
        if dir.as_ref().is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, cb)?;
                } else {
                    cb(&entry);
                }
            }
        }
        Ok(())
    }

    let extension = Some(std::ffi::OsStr::new(extension));
    let mut file_paths = Vec::new();
    visit_dirs(path, &mut |entry| {
        if entry.path().extension() == extension {
            file_paths.push(entry.path());
        }
    })?;
    Ok(file_paths)
}
