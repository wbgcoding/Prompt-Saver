// Windows release: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    prompt_saver_lib::run();
}
