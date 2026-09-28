//! The Places player binary.
//!
//! All engine, renderer and loading code lives in the `places` library so the
//! offline map compiler can share it; this entry point only hands control to
//! [`places::run`].

fn main() -> Result<(), Box<dyn std::error::Error>> {
    places::run()
}
