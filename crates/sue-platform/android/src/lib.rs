//! Sue on Android. Android starts the native activity by calling
//! `android_main` in this library; it keeps the activity for sue-platform
//! and runs Sue's window, the same app as on desktops.
//!
//! On every other system this library is empty.
//!
//! The exported entry is an exception to Sue's forbidden `unsafe`,
//! approved by the maintainer on 2026-10-28 (backbone rule 6), and `java`
//! is the other, approved on 2026-10-29: it wraps the two raw JNI pointers
//! Android hands over, so that sue-platform reaches Java through `jni`'s
//! safe API alone. Anything more needs its own approval.

#[cfg(target_os = "android")]
use sue_platform::android::AndroidApp;
#[cfg(target_os = "android")]
use sue_platform::android::jni::{
    JavaVM,
    objects::{GlobalRef, JObject},
};

#[cfg(target_os = "android")]
#[allow(
    unsafe_code,
    reason = "Android's native activity starts the app by calling this exported symbol; it only hands the activity on and runs Sue"
)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    if let Some((vm, activity)) = java(&app) {
        sue_platform::android::set_java(vm, activity);
    }
    sue_platform::android::set_app(app);
    let code = match std::panic::catch_unwind(sue::run_window) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("sue: {error}");
            1
        }
        Err(_) => 101,
    };
    // Android may start the activity again in this same process (after
    // Back, or after Sue stopped on an error), and a window's event loop
    // can only be made once per process: ending it here makes the next
    // start a fresh one.
    std::process::exit(code);
}

/// Java's VM and Sue's activity, as the safe `jni` types sue-platform
/// calls the system's file pickers with.
#[cfg(target_os = "android")]
#[allow(
    unsafe_code,
    reason = "Android hands over Java's VM and the activity as raw JNI pointers; they are wrapped once here so sue-platform needs no unsafe code"
)]
fn java(app: &AndroidApp) -> Option<(JavaVM, GlobalRef)> {
    // SAFETY: android-activity's pointer is the process's `JavaVM`, valid
    // for as long as the process runs.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let activity = {
        let mut env = vm.attach_current_thread().ok()?;
        // SAFETY: android-activity's pointer is the JNI reference to the
        // activity, valid while it runs; the global reference made from it
        // at once is Sue's own.
        let raw = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        env.new_global_ref(raw).ok()?
    };
    Some((vm, activity))
}
