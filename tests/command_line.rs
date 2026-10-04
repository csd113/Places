//! Invalid native arguments must fail before window creation or filesystem work.
#![cfg(any(unix, windows))]

use std::ffi::OsString;
use std::process::Command;

#[cfg(unix)]
fn invalid_argument() -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(vec![0xff_u8])
}

#[cfg(windows)]
fn invalid_argument() -> OsString {
    use std::os::windows::ffi::OsStringExt;

    OsString::from_wide(&[0xd800_u16])
}

#[test]
fn non_unicode_arguments_return_an_error_before_startup() -> Result<(), std::io::Error> {
    let binaries = [
        (env!("CARGO_BIN_EXE_places"), 1_i32),
        (env!("CARGO_BIN_EXE_places-compile"), 2_i32),
    ];
    for (binary, expected_exit) in binaries {
        let output = Command::new(binary)
            .arg("--help")
            .arg(invalid_argument())
            .output()?;
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{binary} must return its ordinary error status: {diagnostic}"
        );
        assert!(
            diagnostic.contains("not valid UTF-8"),
            "{binary} must explain the argument validation failure: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("panicked"),
            "{binary} must report an input error without a panic: {diagnostic}"
        );
    }
    Ok(())
}
