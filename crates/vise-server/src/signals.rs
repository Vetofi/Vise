
use std::sync::atomic::{AtomicU32, Ordering};

static REQUESTS: AtomicU32 = AtomicU32::new(0);

pub fn requests() -> u32 {
    REQUESTS.load(Ordering::SeqCst)
}

#[cfg(unix)]
pub fn install() {
    extern "C" fn handle(_: libc::c_int) {
        REQUESTS.fetch_add(1, Ordering::SeqCst);
    }
    let handler = handle as extern "C" fn(libc::c_int);
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe {
            libc::signal(signal, handler as libc::sighandler_t);
        }
    }
}

#[cfg(windows)]
pub fn install() {
    unsafe extern "system" fn handle(_: u32) -> windows_sys::core::BOOL {
        REQUESTS.fetch_add(1, Ordering::SeqCst);
        1
    }
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(handle), 1);
    }
}

#[cfg(not(any(unix, windows)))]
pub fn install() {}
