use serialport::{SerialPort, SerialPortInfo, SerialPortType};
use std::{io, time::Duration};

/// Returns a [`Vec`] of the available USB ports. All returned [`SerialPortInfo`]s will have a
/// `port_type` of variant [`SerialPortType::UsbPort`].
///
/// [`Vec`]: Vec
/// [`SerialPortInfo`]: SerialPortInfo
/// [`SerialPortType::UsbPort`]: SerialPortType::UsbPort
pub fn available_usb_ports() -> serialport::Result<Vec<SerialPortInfo>> {
    let ports = serialport::available_ports()?;
    let usb_ports = ports
        .into_iter()
        .filter(|port| match port.port_type {
            SerialPortType::UsbPort(_) => true,
            _ => false,
        })
        .collect();

    Ok(usb_ports)
}

/// Opens the port described by the given [`SerialPortInfo`] for serial read/write at the given
/// `baud`.
///
/// [`SerialPortInfo`]: SerialPortInfo
pub fn open_port(port: &SerialPortInfo, baud: u32) -> serialport::Result<Box<dyn SerialPort>> {
    serialport::new(port.port_name.as_str(), baud)
        .timeout(Duration::from_secs(1))
        .open()
}

fn serial_main() {
    match available_usb_ports() {
        Ok(ports) => {
            for i in 0..ports.len() {
                let name = &ports[i].port_name;
                println!("Port name [{i}]: {name}")
            }

            println!("Select port number: ");

            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer).expect("Expected input");

            let port_number: usize = buffer[0..buffer.len() - 1]
                .parse()
                .expect("Expected a number");

            let selected_port = ports.get(port_number).expect("Expected valid port number");
            let mut port = open_port(selected_port, 9600).expect("Could not open port");

            let mut serial_buffer: [u8; 1024] = [0; 1024];

            loop {
                port.write("Pong".as_bytes())
                    .expect("Should be able to transmit data");

                port.read(&mut serial_buffer).unwrap();
                let serial_str = serial_buffer.as_ascii().unwrap().as_str();
                println!("{serial_str}");
            }
        }

        Err(err) => {
            println!("Failed to read available ports: {err}")
        }
    }
}
