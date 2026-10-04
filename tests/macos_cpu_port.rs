//! The CPU sampler must release each Mach host-port send-right reference.
#![cfg(target_os = "macos")]

unsafe extern "C" {
    static mach_task_self_: u32;
    fn mach_host_self() -> u32;
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
    fn mach_port_get_refs(task: u32, name: u32, right: u32, references: *mut u32) -> i32;
}

/// One fixture-owned send right, independent of the sampler's references.
struct HostPort {
    task: u32,
    host: u32,
}

impl HostPort {
    fn new() -> Self {
        // SAFETY: libSystem initialized this task's stable port before the test.
        let task = unsafe { mach_task_self_ };
        // SAFETY: the no-argument trap returns an owned send-right reference;
        // this fixture releases it in Drop after the last reference query.
        let host = unsafe { mach_host_self() };
        assert_ne!(host, 0_u32, "the native host send right must be available");
        Self { task, host }
    }

    fn references(&self) -> u32 {
        const MACH_PORT_RIGHT_SEND: u32 = 0;
        let mut references = 0_u32;
        // SAFETY: this fixture owns a live host send right in the supplied
        // task, and the exclusively borrowed u32 output outlives the call.
        let status = unsafe {
            mach_port_get_refs(
                self.task,
                self.host,
                MACH_PORT_RIGHT_SEND,
                &raw mut references,
            )
        };
        assert_eq!(status, 0_i32, "Mach send-right reference query failed");
        references
    }
}

impl Drop for HostPort {
    fn drop(&mut self) {
        // SAFETY: the fixture owns exactly one send-right reference and no
        // sampling or reference query is still borrowing it at this point.
        let status = unsafe { mach_port_deallocate(self.task, self.host) };
        assert_eq!(status, 0_i32, "Mach fixture send-right release failed");
    }
}

#[test]
fn repeated_cpu_samples_do_not_accumulate_host_port_references() {
    // An integration-test process isolates the reference count from other
    // unit tests that also sample CPU usage or initialize native GPU devices.
    let port = HostPort::new();
    let before = port.references();
    let mut sampler = places::perf::CpuSampler::new();
    for _ in 0..64_u32 {
        let _utilization = sampler.sample();
    }
    assert_eq!(
        port.references(),
        before,
        "each CPU sample must release the send right it acquired"
    );
}
