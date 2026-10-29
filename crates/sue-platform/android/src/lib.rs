//! Sue on Android. Android starts the native activity by calling
//! `android_main` in this library; it keeps the activity for sue-platform
//! and runs Sue's window, the same app as on desktops.
//!
//! On every other system this library is empty.

#[cfg(target_os = "android")]
#[allow(
    unsafe_code,
    reason = "Android's native activity starts the app by calling this exported symbol; it only hands the activity on and runs Sue"
)]
#[unsafe(no_mangle)]
fn android_main(app: sue_platform::android::AndroidApp) {
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
