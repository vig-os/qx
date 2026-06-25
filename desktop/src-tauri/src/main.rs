//! Binary entry point for the qx desktop shell. All logic
//! lives in the library crate (`lib.rs`) so the command surface stays
//! testable and mobile entry points can reuse it later.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    qx_desktop::run()
}
