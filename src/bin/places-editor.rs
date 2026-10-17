//! Separate native authoring application; no game frame-loop dependency.

fn main() -> std::process::ExitCode {
    places::editor::run()
}
