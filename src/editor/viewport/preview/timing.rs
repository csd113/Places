//! Worker CPU time is distinct from wall time and UI/queue waiting.
#[cfg(target_os = "windows")]
pub(super) fn cpu_ms() -> Option<f64> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn GetThreadTimes(
            thread: *mut std::ffi::c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let mut created = FileTime::default();
    let mut exited = FileTime::default();
    let mut kernel = FileTime::default();
    let mut user = FileTime::default();
    // SAFETY: Windows supplies a valid pseudo-handle for this calling thread.
    let thread = unsafe { GetCurrentThread() };
    // SAFETY: four exclusively borrowed repr(C) FILETIMEs outlive this call.
    let ok = unsafe {
        GetThreadTimes(
            thread,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    };
    if ok == 0_i32 {
        return None;
    }
    let ticks = |value: FileTime| (u64::from(value.high) << 32_u32) | u64::from(value.low);
    Some(
        std::time::Duration::from_nanos(
            ticks(kernel)
                .saturating_add(ticks(user))
                .saturating_mul(100),
        )
        .as_secs_f64()
            * 1_000.0_f64,
    )
}

#[cfg(not(target_os = "windows"))]
pub(super) const fn cpu_ms() -> Option<f64> {
    None
}
