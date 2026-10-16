//! Thin native entry point: legacy geometry commands or the game application.

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
    places::map_compiler::geometry_check::dispatch_cli(&args)?;
    places::game::run(&args)
}
