//! Conservative, local-console monitorability. An SDL focus or frame alone
//! cannot prove an unlocked session, a powered display, or an uncovered view.

use sdl3::{
    sys::{
        properties::SDL_GetPointerProperty,
        video::{SDL_GetWindowProperties, SDL_PROP_WINDOW_WIN32_HWND_POINTER},
    },
    video::Window,
};
use std::{
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromRect,
        },
    },
    System::{
        Power::{
            DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWERBROADCAST_SETTING,
            PowerSettingRegisterNotification, PowerSettingUnregisterNotification,
        },
        RemoteDesktop::{
            NOTIFY_FOR_THIS_SESSION, ProcessIdToSessionId, WTS_CURRENT_SERVER_HANDLE, WTSActive,
            WTSFreeMemory, WTSGetActiveConsoleSessionId, WTSINFOEXW, WTSQuerySessionInformationW,
            WTSRegisterSessionNotification, WTSSessionInfoEx, WTSUnRegisterSessionNotification,
        },
        StationsAndDesktops::{
            CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop,
            UOI_NAME,
        },
        SystemServices::GUID_SESSION_DISPLAY_STATUS,
    },
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            DEVICE_NOTIFY_CALLBACK, GW_HWNDNEXT, GetClientRect, GetSystemMetrics, GetTopWindow,
            GetWindow, GetWindowRect, IsIconic, IsWindowVisible, PBT_APMRESUMEAUTOMATIC,
            PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, SM_CMONITORS, WM_DISPLAYCHANGE,
            WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
        },
    },
};

// Generation and permission bit change in one atomic operation. A callback
// already in flight when we unregister can never publish into a newer generation.
const POWERED: usize = 1;
const EXHAUSTED: usize = usize::MAX & !POWERED;

struct DisplayEvidence(AtomicUsize);

impl DisplayEvidence {
    const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    fn invalidate(&self) {
        let _previous = self
            .0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                (state & !POWERED).checked_add(2)
            });
    }

    fn generation(&self) -> Option<usize> {
        let generation = self.0.load(Ordering::SeqCst) & !POWERED;
        (generation != EXHAUSTED).then_some(generation)
    }

    fn publish(&self, generation: usize, on: bool) -> bool {
        self.0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                (generation != EXHAUSTED && state & !POWERED == generation)
                    .then_some(generation | usize::from(on))
            })
            .is_ok()
    }

    fn powered(&self) -> bool {
        self.0.load(Ordering::SeqCst) & POWERED != 0
    }
}

static DISPLAY: DisplayEvidence = DisplayEvidence::new();
static REFRESH: AtomicBool = AtomicBool::new(false);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
const HOOK: usize = 1;

pub(super) struct Monitor {
    window: HWND,
    power: Option<HPOWERNOTIFY>,
    session: u32,
}

fn bytes<T>() -> u32 {
    u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX)
}

unsafe extern "system" fn window_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_WTSSESSION_CHANGE || message == WM_DISPLAYCHANGE {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }
    if message == WM_DISPLAYCHANGE {
        DISPLAY.invalidate();
        REFRESH.store(true, Ordering::SeqCst);
    }
    if message == WM_POWERBROADCAST {
        let event = u32::try_from(wparam).ok();
        if event == Some(PBT_APMSUSPEND) {
            DISPLAY.invalidate();
            REFRESH.store(false, Ordering::SeqCst);
            INTERRUPTED.store(true, Ordering::SeqCst);
        } else if event == Some(PBT_APMRESUMEAUTOMATIC) {
            DISPLAY.invalidate();
            REFRESH.store(true, Ordering::SeqCst);
            INTERRUPTED.store(true, Ordering::SeqCst);
        }
    }
    // SAFETY: forward the unchanged message while on our window's subclass chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn power_event(
    context: *const std::ffi::c_void,
    kind: u32,
    data: *const std::ffi::c_void,
) -> u32 {
    let on = if kind == PBT_POWERSETTINGCHANGE && !data.is_null() {
        // SAFETY: the registered power-manager callback receives this structure
        // alive for the duration of the call.
        let setting = unsafe { &*data.cast::<POWERBROADCAST_SETTING>() };
        let found = setting.PowerSetting;
        let expected = GUID_SESSION_DISPLAY_STATUS;
        if found.data1 == expected.data1
            && found.data2 == expected.data2
            && found.data3 == expected.data3
            && found.data4 == expected.data4
            && setting.DataLength == bytes::<u32>()
        {
            // SAFETY: DataLength proves four readable payload bytes at Data.
            let state = unsafe { (&raw const setting.Data).cast::<u32>().read_unaligned() };
            state == 1
        } else {
            false
        }
    } else {
        false
    };
    if DISPLAY.publish(context.addr(), on) && !on {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }
    0
}

fn register_power(generation: usize) -> Option<HPOWERNOTIFY> {
    let setting = GUID_SESSION_DISPLAY_STATUS;
    let subscription = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
        Callback: Some(power_event),
        // Opaque generation, passed back verbatim by Windows and never dereferenced.
        Context: std::ptr::without_provenance_mut(generation),
    };
    let mut registration = null_mut();
    // SAFETY: the API reads the subscription for this call and retains only
    // the static function and the opaque context. The out pointer is writable.
    let registered = unsafe {
        PowerSettingRegisterNotification(
            &raw const setting,
            DEVICE_NOTIFY_CALLBACK,
            (&raw const subscription).cast_mut().cast(),
            &raw mut registration,
        )
    };
    (registered == 0 && !registration.is_null()).then_some(registration.addr().cast_signed())
}

fn unregister_power(handle: HPOWERNOTIFY) -> bool {
    // SAFETY: the caller supplies an owned, still-registered power subscription.
    unsafe { PowerSettingUnregisterNotification(handle) == 0 }
}

fn replace_power<T: Copy>(
    current: &mut Option<T>,
    unregister: impl FnOnce(T) -> bool,
    register: impl FnOnce() -> Option<T>,
) {
    if current.is_some_and(|handle| !unregister(handle)) {
        // Keep the old handle for a later retry or Drop. Its evidence is stale.
        return;
    }
    *current = register();
}

impl Monitor {
    pub(super) fn install(window: &Window) -> Option<Self> {
        // SAFETY: window owns the live SDL_Window and its properties.
        let props = unsafe { SDL_GetWindowProperties(window.raw()) };
        // SAFETY: this property specifies the HWND of this live Windows SDL window.
        let hwnd = unsafe {
            SDL_GetPointerProperty(props, SDL_PROP_WINDOW_WIN32_HWND_POINTER, null_mut())
        };
        let mut session = 0;
        if hwnd.is_null() {
            return None;
        }
        // SAFETY: the requested process is the current one and the output is writable.
        if unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session) } == 0_i32
            || session == 0
        {
            return None;
        }
        DISPLAY.invalidate();
        REFRESH.store(false, Ordering::SeqCst);
        INTERRUPTED.store(false, Ordering::SeqCst);
        // SAFETY: this call is on the owning UI thread; no borrowed state enters the hook.
        if unsafe { SetWindowSubclass(hwnd, Some(window_message), HOOK, 0) } == 0_i32 {
            return None;
        }
        // SAFETY: hwnd is live, and only its own WTS session is registered.
        if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } == 0_i32 {
            // SAFETY: remove the subclass just installed on this same thread.
            let _removed = unsafe { RemoveWindowSubclass(hwnd, Some(window_message), HOOK) };
            return None;
        }
        let power = DISPLAY.generation().and_then(register_power);
        if power.is_none() {
            // SAFETY: retract the two successful installations, before returning.
            let _unregistered = unsafe { WTSUnRegisterSessionNotification(hwnd) };
            // SAFETY: this thread installed the subclass above.
            let _removed = unsafe { RemoveWindowSubclass(hwnd, Some(window_message), HOOK) };
            return None;
        }
        Some(Self {
            window: hwnd,
            power,
            session,
        })
    }

    pub(super) const fn session(&self) -> u32 {
        self.session
    }

    pub(super) fn interrupted() -> bool {
        INTERRUPTED.swap(false, Ordering::SeqCst)
    }

    pub(super) fn refresh_pending() -> bool {
        REFRESH.load(Ordering::SeqCst)
    }

    /// The editor loop calls this only after retiring the existing grant.
    pub(super) fn refresh_power(&mut self) {
        if REFRESH.swap(false, Ordering::SeqCst) {
            let generation = DISPLAY.generation();
            replace_power(&mut self.power, unregister_power, || {
                generation.and_then(register_power)
            });
        }
    }

    pub(super) fn verified(&self) -> bool {
        if self.power.is_none() || Self::refresh_pending() || !DISPLAY.powered() {
            return false;
        }
        // SAFETY: query-only console identity. A transition sentinel never equals our session.
        if unsafe { WTSGetActiveConsoleSessionId() } != self.session {
            return false;
        }
        // SAFETY: this is a query-only count. Session display power is not per-monitor evidence.
        if unsafe { GetSystemMetrics(SM_CMONITORS) } != 1_i32
            || !unlocked(self.session)
            || !input_desktop()
        {
            return false;
        }
        let hwnd = self.window;
        // SAFETY: hwnd is retained only while SDL owns the window.
        if unsafe { IsWindowVisible(hwnd) } == 0_i32 {
            return false;
        }
        // SAFETY: same live hwnd.
        if unsafe { IsIconic(hwnd) } != 0_i32 || cloaked(hwnd) != Some(false) {
            return false;
        }
        client_rect(hwnd).is_some_and(|rect| on_display(&rect) && uncovered(hwnd, &rect))
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        DISPLAY.invalidate();
        if let Some(power) = self.power.take() {
            let _power = unregister_power(power);
        }
        // SAFETY: the SDL window outlives the monitor.
        let _session = unsafe { WTSUnRegisterSessionNotification(self.window) };
        // SAFETY: this is the original owning window thread.
        let _subclass = unsafe { RemoveWindowSubclass(self.window, Some(window_message), HOOK) };
    }
}

fn unlocked(session: u32) -> bool {
    let mut result = null_mut();
    let mut size = 0;
    // SAFETY: WTS writes an owned buffer pointer and size to the supplied outputs.
    if unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session,
            WTSSessionInfoEx,
            &raw mut result,
            &raw mut size,
        )
    } == 0_i32
    {
        return false;
    }
    let mut unlocked = false;
    if !result.is_null() && size >= bytes::<WTSINFOEXW>() {
        // SAFETY: successful WTSSessionInfoEx returned at least this many initialized bytes.
        let info = unsafe { result.cast::<WTSINFOEXW>().read_unaligned() };
        if info.Level == 1 {
            // SAFETY: Level 1 selects this union member.
            let data = unsafe { info.Data.WTSInfoExLevel1 };
            unlocked = data.SessionId == session
                && data.SessionState == WTSActive
                && data.SessionFlags == 1_i32;
        }
    }
    // SAFETY: WTS owns the allocator for this successful response.
    unsafe { WTSFreeMemory(result.cast()) };
    unlocked
}

fn input_desktop() -> bool {
    // SAFETY: read-only, non-inheritable access to the desktop receiving input.
    let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desktop.is_null() {
        return false;
    }
    let mut name = [0_u16; 32];
    let mut size = 0;
    // SAFETY: output is writable, aligned and sized exactly as given.
    let read = unsafe {
        GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            bytes::<[u16; 32]>(),
            &raw mut size,
        )
    };
    // SAFETY: OpenInputDesktop returned this unique closeable desktop handle.
    let _closed = unsafe { CloseDesktop(desktop) };
    read != 0_i32
        && name
            .into_iter()
            .take_while(|ch| *ch != 0)
            .eq("Default".encode_utf16())
}

fn cloaked(hwnd: HWND) -> Option<bool> {
    let mut flags = 0_u32;
    // SAFETY: DWMWA_CLOAKED writes a DWORD of exactly this size.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED.cast_unsigned(),
            (&raw mut flags).cast(),
            bytes::<u32>(),
        )
    };
    (result >= 0_i32).then_some(flags != 0)
}

fn client_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    // SAFETY: RECT is valid writable storage.
    if unsafe { GetClientRect(hwnd, &raw mut rect) } == 0_i32 {
        return None;
    }
    let mut start = POINT {
        x: rect.left,
        y: rect.top,
    };
    let mut end = POINT {
        x: rect.right,
        y: rect.bottom,
    };
    // SAFETY: Win32 accesses the two points only during their individual calls.
    let left = unsafe { ClientToScreen(hwnd, &raw mut start) };
    // SAFETY: same live window and exact writable POINT.
    let right = unsafe { ClientToScreen(hwnd, &raw mut end) };
    (left != 0_i32 && right != 0_i32 && start.x < end.x && start.y < end.y).then_some(RECT {
        left: start.x,
        top: start.y,
        right: end.x,
        bottom: end.y,
    })
}

fn on_display(rect: &RECT) -> bool {
    // SAFETY: rect is live; absent monitor must not fall back to a default.
    let monitor = unsafe { MonitorFromRect(&raw const *rect, MONITOR_DEFAULTTONULL) };
    if monitor.is_null() {
        return false;
    }
    let mut info = MONITORINFO {
        cbSize: bytes::<MONITORINFO>(),
        ..MONITORINFO::default()
    };
    // SAFETY: cbSize and the output storage describe the same MONITORINFO type.
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) } == 0_i32 {
        return false;
    }
    let bounds = info.rcMonitor;
    bounds.left <= rect.left
        && bounds.top <= rect.top
        && bounds.right >= rect.right
        && bounds.bottom >= rect.bottom
}

fn uncovered(window: HWND, client: &RECT) -> bool {
    // SAFETY: null requests the first top-level window in Z order.
    let mut above = unsafe { GetTopWindow(null_mut()) };
    for _ in 0_u8..64 {
        if above == window {
            return true;
        }
        if above.is_null() {
            return false;
        }
        // SAFETY: these read-only queries tolerate a window concurrently destroyed.
        let visible = unsafe { IsWindowVisible(above) } != 0_i32;
        // SAFETY: same Z-order snapshot handle, no window data is retained.
        let minimized = unsafe { IsIconic(above) } != 0_i32;
        if visible && !minimized && cloaked(above) != Some(true) {
            let mut rect = RECT::default();
            // SAFETY: output is a writable RECT; failure is an uncertain visibility result.
            if unsafe { GetWindowRect(above, &raw mut rect) } == 0_i32
                || (rect.left < client.right
                    && client.left < rect.right
                    && rect.top < client.bottom
                    && client.top < rect.bottom)
            {
                return false;
            }
        }
        // SAFETY: query the next Z-order entry; an unstable/long list fails closed.
        above = unsafe { GetWindow(above, GW_HWNDNEXT) };
    }
    false
}

#[cfg(test)]
#[path = "../tests/display_power.rs"]
mod tests;
