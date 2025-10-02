#![feature(ascii_char)]

mod gui;
mod serial;
mod stand_state;

fn main() -> eframe::Result {
    gui::start_gui()
}
