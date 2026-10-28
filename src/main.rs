//! Thin native entry point: legacy geometry commands or the game application.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

/// Deliberate command-line use can reuse its caller's console; Explorer never
/// allocates one. Redirected standard handles remain owned by the caller.
#[cfg(all(target_os = "windows", not(debug_assertions)))]
fn attach_command_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    // SAFETY: ATTACH_PARENT_PROCESS is a documented sentinel with no pointers.
    // Failure simply means no parent console exists; no console is allocated.
    let _attached = unsafe { AttachConsole(u32::MAX) };
}

/// Reads native arguments without panicking on an invalid Unicode sequence.
fn command_line_arguments() -> Result<Vec<String>, String> {
    std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<_, _>>()
        .map_err(|invalid_argument| {
            format!(
                "a command-line argument is not valid UTF-8 ({} encoded bytes)",
                invalid_argument.len()
            )
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = command_line_arguments()?;
    #[cfg(all(target_os = "windows", not(debug_assertions)))]
    if !args.is_empty() || places::engine::logging::verbose() {
        attach_command_console();
    }
    places::map_compiler::geometry_check::dispatch_cli(&args)?;
    places::game::run(&args)
}
