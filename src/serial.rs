use serialport::{SerialPort, SerialPortInfo, SerialPortType, UsbPortInfo};
use std::{
    collections::{HashMap, hash_map},
    error::Error,
    fmt::Display,
    io::{self, Read, Write},
    sync::mpsc::{self, Receiver, SendError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::sequence::CommandSequence;

const CHECKED_FIELD_NAMES: [&'static str; 7] = [
    "NP1_OPEN", "NP2_OPEN", "NP3_OPEN", "NP4_OPEN", "IP1_OPEN", "IP2_OPEN", "IP3_OPEN",
];

/// Like [`SerialPortInfo`], but specialized to ports with of type [`SerialPortType::UsbPort`].
/// Since this in encoded in the type of the struct the `port_type` field is omitted, and in its
/// place is an instance of the [`UsbPortInfo`] struct, without need to match on the
/// [`SerialPortType`].
///
/// [`SerialPortInfo`]: SerialPortInfo
/// [`SerialPortType`]: SerialPortType
/// [`SerialPortType::UsbPort`]: SerialPortType::UsbPort
/// [`UsbPortInfo`]: UsbPortInfo
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UsbSerialPortInfo {
    pub port_name: String,
    pub usb_info: UsbPortInfo,
}

impl TryFrom<SerialPortInfo> for UsbSerialPortInfo {
    type Error = NonUsbError;

    fn try_from(value: SerialPortInfo) -> Result<Self, Self::Error> {
        match value.port_type {
            SerialPortType::UsbPort(usb_info) => Ok(Self {
                port_name: value.port_name,
                usb_info,
            }),
            _ => Err(NonUsbError),
        }
    }
}

/// An [`Error`] type for non-USB items which were expected to be USB.
///
/// [`Error`]: Error
#[derive(Eq, PartialEq, Debug)]
pub struct NonUsbError;

impl Display for NonUsbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expected USB but was not USB")
    }
}

impl Error for NonUsbError {}

/// Returns a [`Vec`] of the available USB ports. All returned [`SerialPortInfo`]s will have a
/// `port_type` of variant [`SerialPortType::UsbPort`].
///
/// [`Vec`]: Vec
/// [`SerialPortInfo`]: SerialPortInfo
/// [`SerialPortType::UsbPort`]: SerialPortType::UsbPort
pub fn available_usb_ports() -> serialport::Result<Vec<UsbSerialPortInfo>> {
    let ports = serialport::available_ports()?;
    let usb_ports = ports
        .into_iter()
        .filter_map(|port_info| port_info.try_into().ok())
        .collect();

    Ok(usb_ports)
}

/// Open a USB port described by the given [`UsbSerialPortInfo`] for reading [`SensorField`]s from.
///
/// [`UsbSerialPortInfor`]: UsbSerialPortInfor
/// [`SensorField`]: SensorField
pub fn open_field_port(
    port: &UsbSerialPortInfo,
    baud: u32,
) -> serialport::Result<FieldReader<Box<dyn SerialPort>>> {
    let port = open_port(port, baud)?;
    Ok(FieldReader::new(port))
}

/// Opens the USB port described by the given [`UsbSerialPortInfo`] for serial read/write at the
/// given `baud`.
///
/// [`UsbSerialPortInfo`]: UsbSerialPortInfo
pub fn open_port(port: &UsbSerialPortInfo, baud: u32) -> serialport::Result<Box<dyn SerialPort>> {
    serialport::new(port.port_name.as_str(), baud)
        .timeout(Duration::from_secs(1))
        .open()
}

/// Creates a pair of [`FieldReciever`] and [`FieldSender`], and lets [`FieldSender`] continually
/// read and send [`SensorField`]s from a seperate thread. This function returns the associated
/// [`FieldReciever`] to allow the recieving of read [`SensorField`]s.
///
/// [`SensorField`]: SensorField
/// [`FieldSender`]: FieldSender
/// [`FieldReciever`]: FieldReciever
pub fn start_field_thread<R>(field_reader: FieldReader<R>) -> FieldReciever
where
    R: 'static + Read + Write + Send,
{
    let (field_sender, field_reciever) = field_channel(field_reader);

    thread::spawn(move || {
        let mut field_sender = field_sender;

        loop {
            field_sender.send_fields().expect("Could not read fields");
            field_sender
                .send_commands()
                .expect("Could not send commands");
        }
    });

    field_reciever
}

/// Create a multiple producer single consumer senser reciever channel pair for [`SensorField`]s.
///
/// [`SensorField`]: SensorField
pub fn field_channel<R>(field_reader: FieldReader<R>) -> (FieldSender<R>, FieldReciever)
where
    R: 'static + Read + Write + Send,
{
    let (read_tx, read_rx) = mpsc::channel();
    let (command_tx, command_rx) = mpsc::channel();

    let sender = FieldSender {
        reader: field_reader.reader,
        remainder: field_reader.remainder,
        read_tx,
        command_rx,
    };

    let receiver = FieldReciever {
        fields: field_reader.fields,
        read_rx,
        command_tx,
    };

    (sender, receiver)
}

/// A type for recieving [`SensorField`]s sent over a channel by a [`FieldSender`], which reads
/// [`SensorField`]s.
///
/// [`SensorField`]: SensorField
/// [`FieldSender`]: FieldSender
#[derive(Debug)]
pub struct FieldReciever {
    fields: HashMap<String, SensorValue>,
    read_rx: Receiver<SensorField>,
    command_tx: Sender<Vec<u8>>,
}

/// A wrapper type over a [`Read`] instance for reading [`SensorField`]s and then sending them over
/// a channel to a [`FieldReciever`].
///
/// [`Read`]: Read
/// [`SensorField`]: SensorField
/// [`FieldReciever`]: FieldReciever
#[derive(Debug)]
pub struct FieldSender<R>
where
    R: 'static + Read + Write + Send,
{
    reader: R,
    remainder: String,
    read_tx: Sender<SensorField>,
    command_rx: Receiver<Vec<u8>>,
}

impl FieldReciever {
    /// Gives an [`Iterator`] of the sensor fields of the [`FieldReciever`].
    ///
    /// [`Iterator`]: Iterator
    /// [`FieldReciever`]: FieldReciever
    pub fn fields(&self) -> hash_map::Iter<'_, String, SensorValue> {
        self.fields.iter()
    }

    /// Gets a [`SensorValue`] by its associated [`SensorField`]'s name.
    ///
    /// [`SensorValue`]: SensorValue
    /// [`SensorField`]: SensorField
    pub fn get_field(&self, field_name: &str) -> Option<&SensorValue> {
        self.fields.get(field_name)
    }

    /// Recieve as many fields as possible over the channel without blocking for new
    /// [`SensorField`]s. This function will populate/update the [`FieldReciever`]'s collection
    /// of [`SensorField`]s.
    ///
    /// [`SensorField`]: SensorField
    /// [`FieldReviever`]: FieldReviever
    pub fn recieve_fields(&mut self) {
        while let Ok(field) = self.read_rx.try_recv() {
            self.fields.insert(field.name, field.value);
        }
    }

    /// Send a [`ValveCommand`] to the [`FieldSender`] to be sent down serial.
    ///
    /// [`ValveCommand`]: ValveCommand
    /// [`FieldSender`]: FieldSender
    pub fn send_command(&mut self, command: ValveCommand) -> Result<(), SendError<Vec<u8>>> {
        self.command_tx.send(command.to_string().into_bytes())
    }

    /// Run the given [`CommandSequence`] in the context of the given [`FieldReciever`].
    ///
    /// [`CommandSequence`]: CommandSequence
    /// [`FieldReciever`]: FieldReciever
    pub fn run_sequence(&self, seq: CommandSequence) -> Result<(), SendError<Vec<u8>>> {
        seq.run(self.command_tx.clone())
    }

    /// Run the given [`CommandSequence`] in the context of the given [`FieldReciever`], in a new
    /// thread.
    ///
    /// [`CommandSequence`]: CommandSequence
    /// [`FieldReciever`]: FieldReciever
    pub fn run_sequence_par(
        &self,
        seq: CommandSequence,
    ) -> JoinHandle<Result<(), SendError<Vec<u8>>>> {
        seq.run_par(self.command_tx.clone())
    }
}

impl<R> FieldSender<R>
where
    R: 'static + Read + Write + Send,
{
    /// Read as many [`SensorField`]s as possible from the internal [`Read`] instance and send them
    /// over the channel for the corrosponding [`FieldReciever`].
    ///
    /// [`SensorField`]: SensorField
    /// [`FieldReviever`]: FieldReviever
    /// [`Read`]: Read
    pub fn send_fields(&mut self) -> Result<(), SensorFieldReadError> {
        let (remainder, fields) = read_fields(&mut self.reader, self.remainder.to_owned())?;
        self.remainder = remainder;

        for field in fields {
            self.read_tx
                .send(field)
                .expect("Expected non hung-up reciever");
        }

        Ok(())
    }

    /// Recieve [`ValveCommand`]s from the [`FieldReciever`] and send them down serial.
    ///
    /// [`ValveCommand`]: ValveCommand
    /// [`FieldReciever`]: FieldReciever
    pub fn send_commands(&mut self) -> Result<(), io::Error> {
        let mut commands: Vec<u8> = vec!['\n' as u8];

        while let Ok(mut cmd) = self.command_rx.try_recv() {
            log::info!(
                "Sending command: {}",
                String::from_utf8(cmd.clone()).unwrap().trim()
            );

            commands.append(&mut cmd);
            commands.push('\n' as u8)
        }

        self.reader.write_all(&commands)
    }
}

/// Holds a [`Read`] as well as some internal state for reading out [`SensorField`]s.
///
/// [`Read`]: Read
/// [`SensorField`]: SensorField
#[derive(Debug)]
pub struct FieldReader<R>
where
    R: Read,
{
    reader: R,
    remainder: String,
    fields: HashMap<String, SensorValue>,
}

impl<R> FieldReader<R>
where
    R: Read,
{
    /// Create a new [`SensorFieldReader`] by wrapping the given [`Read`] instance.
    ///
    /// [`SensorFieldReader`]: SensorFieldReader
    /// [`Read`]: Read
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            remainder: String::new(),
            fields: HashMap::new(),
        }
    }

    /// Gives an [`Iterator`] of the sensor fields of the [`SensorFieldReader`].
    ///
    /// [`Iterator`]: Iterator
    /// [`SensoryFieldReader`]: SensorFieldReader
    pub fn fields(&self) -> hash_map::Iter<'_, String, SensorValue> {
        self.fields.iter()
    }

    /// Gets a [`SensorValue`] by its associated [`SensorField`]'s name.
    ///
    /// [`SensorValue`]: SensorValue
    /// [`SensorField`]: SensorField
    pub fn get_field(&self, field_name: &str) -> Option<&SensorValue> {
        self.fields.get(field_name)
    }

    /// Read as many [`SensorField`]s as can be parsed and store/update them in the given
    /// [`SensorFieldReader`].
    ///
    /// [`SensorField`]: SensorField
    /// [`SensorFieldReader`]: SensorFieldReader
    pub fn update_fields(&mut self) -> Result<(), SensorFieldReadError> {
        let fields = self.read_fields()?;

        for SensorField { name, value } in fields {
            self.fields.insert(name, value);
        }

        Ok(())
    }

    /// Read as many [`SensorField`]s as can be parsed from the [`SensorFieldReader`].
    ///
    /// [`SensorField`]: SensorField
    /// [`SensorFieldReader`]: SensorFieldReader
    fn read_fields(&mut self) -> Result<Vec<SensorField>, SensorFieldReadError> {
        // We want to accept textual data of this format:
        //
        // [field name]:[field type abreviation]=[value]\n
        //
        // Where '\n' would indicate the end of the field. Spaces would be acceptable, but would be
        // understood as part of one of the items in square brackets rather than spacing around the
        // colon of equal sign.

        let (remainder, fields) = read_fields(&mut self.reader, self.remainder.to_owned())?;
        self.remainder = remainder;
        Ok(fields)
    }
}

/// Read as many [`SensorField`]s as can be parsed from the given [`Read`]. The [`String`] argument
/// should be the returned [`String`] of the previous call to this function, or an empty [`String`]
/// if this is the first call.
///
/// [`SensorField`]: SensorField
/// [`Read`]: Read
/// [`String`]: String
fn read_fields<R>(
    r: &mut R,
    remainder: String,
) -> Result<(String, Vec<SensorField>), SensorFieldReadError>
where
    R: Read,
{
    const MAX_READ_RETRYS: u32 = 16;

    let mut buf: [u8; 1024] = [0; 1024];

    for i in 0..=MAX_READ_RETRYS {
        match r.read(&mut buf) {
            Ok(_) => break,
            Err(e) if i != MAX_READ_RETRYS && e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(SensorFieldReadError::IoError(e)),
        }
    }

    let filtered_buf: Vec<u8> = buf.into_iter().filter(|&b| b != 0).collect();
    let read_text = match String::from_utf8(filtered_buf) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Bad serial read: {e}");
            String::default()
        }
    };

    // Append the previous iteration's remainder in order to complete the first line.
    let text = format!("{remainder}{read_text}");

    // Remove the last line since it might not be a complete field, which would cause a parse error.
    let (lines, remainder) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
    let fields = lines
        .lines()
        .map(|line| parse_sensor_field(line))
        .filter_map(Result::ok)
        .filter(|field| CHECKED_FIELD_NAMES.contains(&field.name.as_str()))
        .collect();

    Ok((remainder.to_string(), fields))
}

#[derive(Debug)]
pub enum SensorFieldReadError {
    IoError(io::Error),
}

impl Display for SensorFieldReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensorFieldReadError::IoError(e) => write!(f, "Failed to read fields: IO error: {e}"),
        }
    }
}

impl Error for SensorFieldReadError {}

pub const NILE_VALVE_NP1: &'static str = "NP1";
pub const NILE_VALVE_NP2: &'static str = "NP2";
pub const NILE_VALVE_NP3: &'static str = "NP3";
pub const NILE_VALVE_NP4: &'static str = "NP4";

pub const NILE_VALVE_IP1: &'static str = "IP1";
pub const NILE_VALVE_IP2: &'static str = "IP2";
pub const NILE_VALVE_IP3: &'static str = "IP3";

/// A command for actuating valves on NILE.
#[derive(Clone, Eq, PartialEq)]
pub enum ValveCommand {
    /// Open a valve with the given name.
    Open(&'static str),

    /// Close a valve with the given name.
    Close(&'static str),
}

/// A field, presumably transmitted over serial representing the reading of a sensor on the NILE
/// stand.
#[derive(Debug, PartialEq, Clone)]
pub struct SensorField {
    pub name: String,
    pub value: SensorValue,
}

/// A value from a sensor. Includes basic primitives
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SensorValue {
    UnsignedInt(u64),
    SignedInt(i64),
    Float(f64),
    Boolean(bool),
}

impl ValveCommand {
    /// Serialize the [`ValveCommand`] into a [`String`].
    ///
    /// [`ValveCommand`]: ValveCommand
    /// [`String`]: String
    fn to_string(self) -> String {
        match self {
            ValveCommand::Open(name) => format!("\nOPEN:{name}\n"),
            ValveCommand::Close(name) => format!("\nCLOSE:{name}\n"),
        }
    }
}

impl Display for SensorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensorValue::UnsignedInt(v) => v.fmt(f),
            SensorValue::SignedInt(v) => v.fmt(f),
            SensorValue::Float(v) => v.fmt(f),
            SensorValue::Boolean(v) => v.fmt(f),
        }
    }
}

/// Errors that can occur while parsing either a [`SensorValue`] or [`SensorField`].
///
/// [`SensorValue`]: SensorValue
/// [`SensorField`]: SensorField
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FieldParseError {
    MissingValue,
    MissingType,
    MissingName,
    InvalidType(String),
    InvalidValue(String),
    ToManyTokens,
}

impl Display for FieldParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not parse sensor field: ")?;
        match self {
            FieldParseError::MissingValue => write!(f, "Missing field value"),
            FieldParseError::MissingType => write!(f, "Missing field type"),
            FieldParseError::MissingName => write!(f, "Missing field name"),
            FieldParseError::InvalidType(token) => write!(f, "Invalie field type: {token}"),
            FieldParseError::InvalidValue(token) => write!(f, "Invalid value: '{token}'"),
            FieldParseError::ToManyTokens => write!(f, "To many tokens in field"),
        }
    }
}

impl Error for FieldParseError {}

/// Parses a [`SensorField`] in this format:
///
/// `[name]:[type]=[value]`
///
/// See [`serial::parse_sensor_value`] for more info on the `[type]` and `[value]` parts.
///
/// `[name]` may be any text not including the ':' and '=' characters used as delimeters and also
/// not including a newline.
///
/// [`SensorField`]: SensorField
/// [`serial::parse_sensor_value`]: parse_sensor_value
fn parse_sensor_field(s: &str) -> Result<SensorField, FieldParseError> {
    let tokens: Vec<&str> = s.split(':').collect();

    if tokens.len() > 2 {
        return Err(FieldParseError::ToManyTokens);
    }

    let name: String = tokens
        .first()
        .ok_or(FieldParseError::MissingName)?
        .trim()
        .chars()
        .filter(|&c| c != '\0')
        .collect();
    let value_token: String = tokens
        .get(1)
        .ok_or(FieldParseError::MissingType)?
        .trim()
        .chars()
        .filter(|&c| c != '\0')
        .collect();

    Ok(SensorField {
        name: name.to_string(),
        value: parse_sensor_value(&value_token)?,
    })
}

/// Parses a [`SensorValue`] from a string in this format:
///
/// `[type]=[value]`
///
/// `[type]` should be one of 'u', 'i', 'f', or 'b', where each letter represents unsigned integers,
/// signed integers, floats, and booleans respectively. For each type the `[value]` should be a
/// literal in the typicaly format for Rust (i.e. `123`, `-2`, `0.3`, and `false` respectively).
///
/// All numeric types get run through Rust's [`str::parse`] function, booleans are simply matched
/// against `true` and `false`. Booleans here are not case sensitive.
///
/// [`SensorValue`]: SensorValue
/// [`str::parse`]: str::parse
fn parse_sensor_value(s: &str) -> Result<SensorValue, FieldParseError> {
    let tokens: Vec<&str> = s.split('=').collect();

    if tokens.len() > 2 {
        return Err(FieldParseError::ToManyTokens);
    }

    let type_token = *tokens.first().ok_or(FieldParseError::MissingType)?;
    let value_token = *tokens.get(1).ok_or(FieldParseError::MissingValue)?;

    match type_token {
        "u" => {
            let value = value_token
                .parse()
                .map_err(|_| FieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::UnsignedInt(value))
        }

        "i" => {
            let value = value_token
                .parse()
                .map_err(|_| FieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::SignedInt(value))
        }

        "f" => {
            let value = value_token
                .parse()
                .map_err(|_| FieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::Float(value))
        }

        "b" => {
            let value = match value_token.to_lowercase().as_str() {
                "true" => true,
                "false" => false,
                _ => return Err(FieldParseError::InvalidValue(value_token.to_string())),
            };

            Ok(SensorValue::Boolean(value))
        }

        _ => Err(FieldParseError::InvalidType(type_token.to_string())),
    }
}
