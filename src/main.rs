#![feature(ascii_char)]
#![feature(iterator_try_collect)]

use serialport::SerialPort;
use std::{
    io::{self, Write},
    process::exit,
};

#[cfg(not(feature = "sim_io"))]
use crate::serial::start_field_thread;

#[cfg(feature = "sim_io")]
use crate::serial::start_simulation_field_thread;

mod diagram;
mod field_history;
mod gui;
mod record;
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

    #[cfg(feature = "sim_io")]
    {
        let sim_device = sim_field_io(b"NP1_OPEN:b=TRUE\nPT0:f=3.1415\n");
        let field_rx = start_simulation_field_thread(sim_device);
        gui::start_gui(field_rx)
    }

    #[cfg(not(feature = "sim_io"))]
    {
        let io_device = get_field_io_device();
        let field_rx = start_field_thread(io_device);
        gui::start_gui(field_rx)
    }
}

/// Creates a dumby simulation [`FieldIO`] device which just reads off the given slice.
///
/// [`FieldIO`]: FieldIO
#[cfg(feature = "sim_io")]
fn sim_field_io<'a>(buf: &'a [u8]) -> serial::FieldIO<&'a [u8]> {
    serial::FieldIO::new(buf)
}

/// Prompt the user to select one of the available USB serial connections and return it. This
/// function handles errors itself, logging them and exiting the program as a whole.
#[cfg(not(feature = "sim_io"))]
fn get_field_io_device() -> serial::FieldIO<Box<dyn SerialPort>> {
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

        println!("\tPort name [{i}]: {} ({})", name, port.port_name);
    }

    write!(io::stdout(), "Select Port number (enter 'r' to refresh): ").unwrap();
    io::stdout().flush().unwrap();

    let mut buffer = String::new();

    match io::stdin().read_line(&mut buffer) {
        Ok(n) => log::debug!("Read {n} bytes from stdin"),
        Err(err) => {
            log::error!("Failed to read from stdin: {err}");
            exit(1);
        }
    };

    if buffer.as_str() == "r\n" {
        return get_field_io_device();
    }

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

    log::info!("Established serial connection!");
    field_reader
}
