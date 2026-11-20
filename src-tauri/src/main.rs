// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same executable runs PDFium in an isolated process; see `pdf_worker`.
    if let Some(code) = easy_converter_lib::pdf_worker::run_if_worker() {
        std::process::exit(code);
    }
    easy_converter_lib::run()
}
