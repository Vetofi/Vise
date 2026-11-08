
use std::sync::atomic::{AtomicBool, Ordering};

pub fn favour_current() -> bool {
    if std::thread::available_parallelism().map_or(1, |n| n.get()) <= 4 {
        return false;
    }
    favour()
}

#[cfg(target_vendor = "apple")]
fn favour() -> bool {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0) == 0
    }
}

#[cfg(windows)]
fn favour() -> bool {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
    };
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL) != 0 }
}

#[cfg(not(any(target_vendor = "apple", windows)))]
fn favour() -> bool {
    false
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub fn thread_cpu() -> Option<std::time::Duration> {
    use std::{
        sync::OnceLock,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::System::{
        Threading::GetCurrentThread, WindowsProgramming::QueryThreadCycleTime,
    };
    static RATE: OnceLock<f64> = OnceLock::new();
    let rate = *RATE.get_or_init(|| {
        let tsc = || unsafe { core::arch::x86_64::_rdtsc() };
        let (wall, start) = (Instant::now(), tsc());
        std::thread::sleep(Duration::from_millis(20));
        (tsc() - start) as f64 / wall.elapsed().as_nanos() as f64
    });
    let mut cycles = 0;
    if unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) } == 0 {
        return None;
    }
    Some(Duration::from_nanos((cycles as f64 / rate) as u64))
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
pub fn thread_cpu() -> Option<std::time::Duration> {
    None
}

static HELPERS_BEHIND: AtomicBool = AtomicBool::new(false);

#[cfg(any(target_os = "linux", target_os = "android"))]
const BEHIND: i32 = 10;

pub fn put_helpers_behind() {
    HELPERS_BEHIND.store(true, Ordering::Relaxed);
}

pub fn behind_server() -> bool {
    HELPERS_BEHIND.load(Ordering::Relaxed) && behind()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn behind() -> bool {
    unsafe {
        let nice = libc::getpriority(libc::PRIO_PROCESS as _, 0);
        if nice + BEHIND <= 19 {
            libc::setpriority(libc::PRIO_PROCESS as _, 0, nice + BEHIND) == 0
        } else {
            let param = libc::sched_param { sched_priority: 0 };
            libc::sched_setscheduler(0, libc::SCHED_IDLE, &param) == 0
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn behind() -> bool {
    false
}

pub fn idle_current() -> bool {
    idle()
}

#[cfg(target_os = "linux")]
fn idle() -> bool {
    let param = libc::sched_param { sched_priority: 0 };
    unsafe { libc::sched_setscheduler(0, libc::SCHED_IDLE, &param) == 0 }
}

#[cfg(target_vendor = "apple")]
fn idle() -> bool {
    unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0) == 0 }
}

#[cfg(windows)]
fn idle() -> bool {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_IDLE,
    };
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_IDLE) != 0 }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple", windows)))]
fn idle() -> bool {
    false
}

pub fn stop_requests() -> u32 {
    STOP_REQUESTS.load(std::sync::atomic::Ordering::SeqCst)
}

static STOP_REQUESTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
pub fn catch_stop_signals() {
    extern "C" fn handle(_: libc::c_int) {
        if STOP_REQUESTS.fetch_add(1, Ordering::SeqCst) >= 1 {
            unsafe { libc::_exit(130) }
        }
    }
    let handler = handle as extern "C" fn(libc::c_int);
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe {
            libc::signal(signal, handler as libc::sighandler_t);
        }
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
pub fn catch_stop_signals() {}

pub use mimalloc::MiMalloc as Allocator;

const PURGE_DELAY: libmimalloc_sys::mi_option_t = 15;

pub fn configure_allocator() {
    unsafe {
        libmimalloc_sys::mi_option_set(PURGE_DELAY, 0);
    }
}

pub fn release_free_memory() {
    unsafe {
        libmimalloc_sys::mi_collect(true);
    }
    release();
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn release() {
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(target_vendor = "apple")]
fn release() {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }
    unsafe {
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
}

#[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_vendor = "apple")))]
fn release() {}

#[cfg(test)]
mod tests {
    #[test]
    fn favouring_a_thread_takes_where_the_os_allows_it() {
        let took = std::thread::spawn(super::favour).join().unwrap();
        assert_eq!(took, cfg!(any(target_vendor = "apple", windows)));
        #[cfg(target_vendor = "apple")]
        std::thread::spawn(|| {
            assert!(super::favour());
            let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
            let mut relative = 0;
            let read = unsafe {
                libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut relative)
            };
            assert_eq!(read, 0);
            assert!(matches!(
                class,
                libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE
            ));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn helpers_go_behind_only_when_asked() {
        assert!(!std::thread::spawn(super::behind_server).join().unwrap());
        super::put_helpers_behind();
        #[cfg(any(target_os = "linux", target_os = "android"))]
        std::thread::spawn(|| {
            let before = unsafe { libc::getpriority(libc::PRIO_PROCESS as _, 0) };
            assert!(super::behind_server());
            let (after, policy) = unsafe {
                (
                    libc::getpriority(libc::PRIO_PROCESS as _, 0),
                    libc::sched_getscheduler(0),
                )
            };
            if before + super::BEHIND <= 19 {
                assert_eq!(after, before + super::BEHIND);
            } else {
                assert_eq!(policy, libc::SCHED_IDLE);
            }
        })
        .join()
        .unwrap();
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        assert!(!std::thread::spawn(super::behind_server).join().unwrap());
    }

    #[test]
    fn idling_a_thread_takes() {
        let took = std::thread::spawn(super::idle_current).join().unwrap();
        assert_eq!(
            took,
            cfg!(any(target_os = "linux", target_vendor = "apple", windows))
        );
        #[cfg(target_os = "linux")]
        std::thread::spawn(|| {
            assert!(super::idle_current());
            assert_eq!(unsafe { libc::sched_getscheduler(0) }, libc::SCHED_IDLE);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn configuring_the_allocator_purges_at_once() {
        let get = || unsafe { libmimalloc_sys::mi_option_get(super::PURGE_DELAY) };
        if std::env::var_os("MIMALLOC_PURGE_DELAY").is_none() {
            assert_eq!(get(), 1000);
        }
        super::configure_allocator();
        assert_eq!(get(), 0);
    }
}
