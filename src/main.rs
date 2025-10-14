#![feature(ascii_char)]
#![feature(iterator_try_collect)]

use serialport::SerialPort;
use std::{io, process::exit};

mod gui;
mod sequence;
mod serial;
mod stand;

fn main() -> eframe::Result {
    simplelog::TermLogger::init(
        log::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("Could not initialize logging");

    gui::start_gui(get_field_reader())
}

/// Prompt the user to select one of the available USB serial connections and return it. This
/// function handles errors itself, logging them and exiting the program as a whole.
fn get_field_reader() -> serial::FieldReader<Box<dyn SerialPort>> {
    let usb_ports = match serial::available_usb_ports() {
        Ok(ports) => ports,

        Err(err) => {
            log::error!("Could not identify available USB ports: {err}");
            exit(1);
        }
    };

    println!("Available ports:");

    for i in 0..usb_ports.len() {
        let port = &usb_ports[i];
        let name = match &port.usb_info.product {
            None => "",
            Some(s) => s,
        };

        println!("\tPort name [{i}]: {} ({})", name, port.port_name)
    }

    println!("Select port number: ");

    let mut buffer = String::new();

    match io::stdin().read_line(&mut buffer) {
        Ok(n) => log::debug!("Read {n} bytes from stdin"),
        Err(err) => {
            log::error!("Failed to read from stdin: {err}");
            exit(1);
        }
    };

    let port_number: Option<usize> = buffer.trim().parse().ok();
    let selected_port = match port_number.and_then(|n| usb_ports.get(n)) {
        Some(port) => port,
        None => {
            println!("Please enter the port number of a port in the above list");
            exit(1);
        }
    };

    let field_reader = match serial::open_field_port(selected_port, 115200) {
        Ok(p) => p,
        Err(err) => {
            log::error!("Could not open the selected port: {err}");
            exit(1);
        }
    };

    field_reader
}
