//! In-process platform counters used only by the visible overlay's 500ms tick.

/// Device-wide activity, distinct from this game's timestamped render work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuUtilization {
    Measured(f32),
    Unavailable(&'static str),
}

/// Platform resources are discovered lazily and retained for the overlay.
#[cfg_attr(target_os = "macos", derive(Default))]
pub struct DeviceSampler {
    #[cfg(target_os = "macos")]
    macos: macos::DeviceSampler,
    #[cfg(target_os = "linux")]
    devfreq_root: &'static str,
    #[cfg(target_os = "linux")]
    drm_root: &'static str,
    #[cfg(target_os = "linux")]
    debugfs_root: &'static str,
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    unavailable_reason: &'static str,
}

#[cfg(not(target_os = "macos"))]
impl Default for DeviceSampler {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            macos: macos::DeviceSampler::default(),
            #[cfg(target_os = "linux")]
            devfreq_root: "/sys/class/devfreq",
            #[cfg(target_os = "linux")]
            drm_root: "/sys/class/drm",
            #[cfg(target_os = "linux")]
            debugfs_root: "/sys/kernel/debug",
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            unavailable_reason: "device counter unsupported",
        }
    }
}

impl DeviceSampler {
    pub(crate) fn sample_gpu(&mut self) -> GpuUtilization {
        #[cfg(target_os = "macos")]
        {
            self.macos.sample()
        }
        #[cfg(target_os = "linux")]
        {
            super::sample_gpu_from_paths(
                std::path::Path::new(self.devfreq_root),
                std::path::Path::new(self.drm_root),
                std::path::Path::new(self.debugfs_root),
            )
            .map_or(
                GpuUtilization::Unavailable("device counter unavailable"),
                GpuUtilization::Measured,
            )
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            GpuUtilization::Unavailable(self.unavailable_reason)
        }
    }
}

/// Current process resident bytes; excludes virtual address space and does not
/// attempt to add driver-owned GPU allocations to unified-memory accounting.
pub(super) fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        macos::resident_bytes()
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| parse_linux_rss(&status))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_rss(status: &str) -> Option<u64> {
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?;
    let mut fields = value.split_whitespace();
    let kibibytes = fields.next()?.parse::<u64>().ok()?;
    if fields.next()? != "kB" || fields.next().is_some() {
        return None;
    }
    kibibytes.checked_mul(1024)
}

#[cfg(any(target_os = "macos", test))]
fn valid_utilization(value: f32) -> Option<f32> {
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, c_char, c_void};
    use std::num::NonZeroU32;
    use std::ptr::NonNull;

    use super::{GpuUtilization, valid_utilization};

    // ABI declarations checked against the installed macOS SDK's public
    // IOKitLib.h, CFBase/Dictionary/Number/String.h and libproc/proc_info.h.
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        #[link_name = "IOServiceMatching"]
        fn io_service_matching(name: *const c_char) -> *const c_void;
        #[link_name = "IOServiceGetMatchingService"]
        fn io_service_get_matching_service(port: u32, matching: *const c_void) -> u32;
        #[link_name = "IORegistryEntryCreateCFProperty"]
        fn io_registry_entry_create_cf_property(
            entry: u32,
            key: *const c_void,
            allocator: *const c_void,
            options: u32,
        ) -> *const c_void;
        #[link_name = "IOObjectRelease"]
        fn io_object_release(object: u32) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        #[link_name = "CFStringCreateWithCString"]
        fn cf_string_create_with_c_string(
            allocator: *const c_void,
            string: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        #[link_name = "CFGetTypeID"]
        fn cf_get_type_id(value: *const c_void) -> usize;
        #[link_name = "CFDictionaryGetTypeID"]
        fn cf_dictionary_get_type_id() -> usize;
        #[link_name = "CFNumberGetTypeID"]
        fn cf_number_get_type_id() -> usize;
        #[link_name = "CFDictionaryGetValue"]
        fn cf_dictionary_get_value(dictionary: *const c_void, key: *const c_void) -> *const c_void;
        #[link_name = "CFNumberGetValue"]
        fn cf_number_get_value(number: *const c_void, kind: isize, output: *mut c_void) -> u8;
        #[link_name = "CFRelease"]
        fn cf_release(value: *const c_void);
    }

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut c_void, bytes: i32) -> i32;
    }

    /// Owns exactly one retained CF Create result; dictionary values stay borrowed.
    struct OwnedCf(NonNull<c_void>);

    impl OwnedCf {
        fn new(value: *const c_void) -> Option<Self> {
            NonNull::new(value.cast_mut()).map(Self)
        }

        fn string(value: &CStr) -> Option<Self> {
            const UTF8_ENCODING: u32 = 0x0800_0100;
            // SAFETY: the CStr is NUL-terminated and lives through the call;
            // NULL selects the default allocator and the SDK's UTF-8 encoding.
            Self::new(unsafe {
                cf_string_create_with_c_string(std::ptr::null(), value.as_ptr(), UTF8_ENCODING)
            })
        }

        const fn as_ptr(&self) -> *const c_void {
            self.0.as_ptr().cast_const()
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            // SAFETY: this wrapper owns a non-NULL retained Create result and
            // releases it exactly once; borrowed dictionary values never enter it.
            unsafe { cf_release(self.as_ptr()) };
        }
    }

    struct Service(NonZeroU32);

    impl Drop for Service {
        fn drop(&mut self) {
            // SAFETY: GetMatchingService returned this owned nonzero IO object;
            // no other owner releases it and no sampling occurs during Drop.
            let _release_status = unsafe { io_object_release(self.0.get()) };
        }
    }

    struct AgxCounter {
        service: Service,
        statistics_key: OwnedCf,
        utilization_key: OwnedCf,
        dictionary_type: usize,
        number_type: usize,
    }

    impl AgxCounter {
        fn new() -> Option<Self> {
            // SAFETY: this constant class name is a live NUL-terminated CStr.
            let matching = unsafe { io_service_matching(c"AGXAccelerator".as_ptr()) };
            if matching.is_null() {
                return None;
            }
            // SAFETY: matching is a retained matching dictionary. IOKit
            // consumes its reference on every result; NULL port means default.
            let raw_service = unsafe { io_service_get_matching_service(0, matching) };
            let service = Service(NonZeroU32::new(raw_service)?);
            let statistics_key = OwnedCf::string(c"PerformanceStatistics")?;
            let utilization_key = OwnedCf::string(c"Device Utilization %")?;
            // SAFETY: CF type-id accessors take no pointers and return process-stable IDs.
            let dictionary_type = unsafe { cf_dictionary_get_type_id() };
            // SAFETY: CF type-id accessors take no pointers and return process-stable IDs.
            let number_type = unsafe { cf_number_get_type_id() };
            Some(Self {
                service,
                statistics_key,
                utilization_key,
                dictionary_type,
                number_type,
            })
        }

        fn sample(&self) -> GpuUtilization {
            // SAFETY: the cached service and CFString key remain owned/live;
            // the default allocator and zero options select the public property read.
            let raw_property = unsafe {
                io_registry_entry_create_cf_property(
                    self.service.0.get(),
                    self.statistics_key.as_ptr(),
                    std::ptr::null(),
                    0,
                )
            };
            let Some(property) = OwnedCf::new(raw_property) else {
                return GpuUtilization::Unavailable("statistics unavailable");
            };
            // SAFETY: property owns a non-NULL CF object for this scope.
            let property_type = unsafe { cf_get_type_id(property.as_ptr()) };
            if property_type != self.dictionary_type {
                return GpuUtilization::Unavailable("statistics type unsupported");
            }
            // SAFETY: the type was checked as CFDictionary; the CFString key
            // remains live. This value is borrowed while property stays owned.
            let number = unsafe {
                cf_dictionary_get_value(property.as_ptr(), self.utilization_key.as_ptr())
            };
            if number.is_null() {
                return GpuUtilization::Unavailable("device counter unavailable");
            }
            // SAFETY: the non-NULL dictionary value is retained by property.
            let number_type = unsafe { cf_get_type_id(number) };
            if number_type != self.number_type {
                return GpuUtilization::Unavailable("counter type unsupported");
            }
            let mut utilization = 0.0_f32;
            // SAFETY: number is a verified live CFNumber; Float32 type=5 writes
            // exactly one f32 into the exclusively borrowed output. Boolean is u8.
            let converted =
                unsafe { cf_number_get_value(number, 5, (&raw mut utilization).cast()) };
            if converted == 0 {
                return GpuUtilization::Unavailable("counter conversion failed");
            }
            valid_utilization(utilization).map_or(
                GpuUtilization::Unavailable("counter outside 0..100"),
                GpuUtilization::Measured,
            )
        }
    }

    #[derive(Default)]
    enum SamplerState {
        #[default]
        Uninitialized,
        Available(AgxCounter),
        Unavailable,
    }

    #[derive(Default)]
    pub(super) struct DeviceSampler {
        state: SamplerState,
    }

    impl DeviceSampler {
        pub(super) fn sample(&mut self) -> GpuUtilization {
            if matches!(self.state, SamplerState::Uninitialized) {
                self.state =
                    AgxCounter::new().map_or(SamplerState::Unavailable, SamplerState::Available);
            }
            match &self.state {
                SamplerState::Available(counter) => counter.sample(),
                SamplerState::Unavailable | SamplerState::Uninitialized => {
                    GpuUtilization::Unavailable("no AGX device counter")
                }
            }
        }
    }

    /// SDK `proc_taskinfo`: six `uint64_t` fields followed by twelve `int32_t` fields.
    #[repr(C)]
    #[derive(Default)]
    struct ProcTaskInfo {
        _virtual_size: u64,
        resident_size: u64,
        _total_user: u64,
        _total_system: u64,
        _threads_user: u64,
        _threads_system: u64,
        _policy: i32,
        _faults: i32,
        _pageins: i32,
        _cow_faults: i32,
        _messages_sent: i32,
        _messages_received: i32,
        _syscalls_mach: i32,
        _syscalls_unix: i32,
        _context_switches: i32,
        _thread_count: i32,
        _running_threads: i32,
        _priority: i32,
    }

    // Compile-time ABI checks against public proc_info.h; no GPU/device test setup.
    const _: [(); 96] = [(); std::mem::size_of::<ProcTaskInfo>()];
    const _: [(); 8] = [(); std::mem::offset_of!(ProcTaskInfo, resident_size)];

    pub(super) fn resident_bytes() -> Option<u64> {
        const PROC_PIDTASKINFO: i32 = 4;
        let pid = i32::try_from(std::process::id()).ok()?;
        let bytes = i32::try_from(std::mem::size_of::<ProcTaskInfo>()).ok()?;
        let mut info = ProcTaskInfo::default();
        // SAFETY: PROC_PIDTASKINFO selects this checked repr(C) layout. The
        // initialized output has exactly `bytes` writable bytes, lives through
        // the synchronous call, and the PID is this process. No privileges needed.
        let read = unsafe { proc_pidinfo(pid, PROC_PIDTASKINFO, 0, (&raw mut info).cast(), bytes) };
        (read == bytes).then_some(info.resident_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_memory_parser_uses_rss_kibibytes_and_checks_units_and_overflow() {
        // A small status string guards against reporting virtual memory as
        // game RAM, unit mistakes and overflow. No process/GPU fixtures needed.
        assert_eq!(
            parse_linux_rss("VmSize: 999999 kB\nVmRSS: 2048 kB\n"),
            Some(2_097_152)
        );
        assert_eq!(parse_linux_rss("VmSize: 999999 kB\n"), None);
        assert_eq!(parse_linux_rss("VmRSS: 12 MB\n"), None);
        assert_eq!(parse_linux_rss("VmRSS: 18446744073709551615 kB\n"), None);
        for value in [f32::NAN, f32::INFINITY, -1.0, 101.0] {
            assert_eq!(
                valid_utilization(value),
                None,
                "invalid device counter {value}"
            );
        }
        assert_eq!(valid_utilization(0.0), Some(0.0));
        assert_eq!(valid_utilization(100.0), Some(100.0));
    }
}
