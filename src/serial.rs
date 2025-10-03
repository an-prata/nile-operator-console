use serialport::{SerialPort, SerialPortInfo, SerialPortType, UsbPortInfo};
use std::{
    collections::{HashMap, hash_map},
    error::Error,
    fmt::Display,
    io::{self, Read},
    string::FromUtf8Error,
    time::Duration,
};

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
) -> serialport::Result<SensorFieldReader<Box<dyn SerialPort>>> {
    let port = open_port(port, baud)?;
    Ok(SensorFieldReader::new(port))
}

/// Create a simulated [`SensorFieldReader`] by using a pre-filled slice of bytes as the input.
///
/// [`SensorFieldReader`]: SensorFieldReader
pub fn field_port_sim(buffer: &[u8]) -> SensorFieldReader<&[u8]> {
    SensorFieldReader::new(buffer)
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

/// Holds a [`Read`] as well as some internal state for reading out [`SensorField`]s.
///
/// [`Read`]: Read
/// [`SensorField`]: SensorField
#[derive(Debug, Clone)]
pub struct SensorFieldReader<R>
where
    R: Read,
{
    reader: R,
    remainder: String,
    fields: HashMap<String, SensorValue>,
}

impl<R> SensorFieldReader<R>
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
    let mut buf: [u8; 1024] = [0; 1024];

    r.read(&mut buf)
        .map_err(|e| SensorFieldReadError::IoError(e))?;

    let read_text =
        String::from_utf8(buf.to_vec()).map_err(|e| SensorFieldReadError::Utf8Error(e))?;

    // Append the previous iteration's remainder in order to complete the first line.
    let text = format!("{remainder}{read_text}");
    let mut lines: Vec<&str> = text.lines().collect();

    // Remove the last line since it might not be a complete field, which would cause a parse error.
    let remainder = lines.pop().map(str::to_string).unwrap_or(String::new());
    let fields = lines
        .into_iter()
        .map(|line| parse_sensor_field(line))
        .try_collect()
        .map_err(|e| SensorFieldReadError::ParseError(e))?;

    Ok((remainder, fields))
}

#[derive(Debug)]
pub enum SensorFieldReadError {
    ParseError(SensorFieldParseError),
    IoError(io::Error),
    Utf8Error(FromUtf8Error),
}

impl Display for SensorFieldReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensorFieldReadError::ParseError(e) => write!(f, "Failed to read fields: {e}"),
            SensorFieldReadError::IoError(e) => write!(f, "Failed to read fields: IO error: {e}"),
            SensorFieldReadError::Utf8Error(e) => {
                write!(f, "Failed to read fields: Utf8 error: {e}")
            }
        }
    }
}

impl Error for SensorFieldReadError {}

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
pub enum SensorFieldParseError {
    MissingValue,
    MissingType,
    MissingName,
    InvalidType(String),
    InvalidValue(String),
    ToManyTokens,
}

impl Display for SensorFieldParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not parse sensor field: ")?;
        match self {
            SensorFieldParseError::MissingValue => write!(f, "Missing field value"),
            SensorFieldParseError::MissingType => write!(f, "Missing field type"),
            SensorFieldParseError::MissingName => write!(f, "Missing field name"),
            SensorFieldParseError::InvalidType(token) => write!(f, "Invalie field type: {token}"),
            SensorFieldParseError::InvalidValue(token) => write!(f, "Invalid value: '{token}'"),
            SensorFieldParseError::ToManyTokens => write!(f, "To many tokens in field"),
        }
    }
}

impl Error for SensorFieldParseError {}

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
fn parse_sensor_field(s: &str) -> Result<SensorField, SensorFieldParseError> {
    let tokens: Vec<&str> = s.split(':').collect();

    if tokens.len() > 2 {
        return Err(SensorFieldParseError::ToManyTokens);
    }

    let name = tokens.first().ok_or(SensorFieldParseError::MissingName)?;
    let value_token = tokens.get(1).ok_or(SensorFieldParseError::MissingType)?;

    Ok(SensorField {
        name: name.to_string(),
        value: parse_sensor_value(value_token)?,
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
fn parse_sensor_value(s: &str) -> Result<SensorValue, SensorFieldParseError> {
    let tokens: Vec<&str> = s.split('=').collect();

    if tokens.len() > 2 {
        return Err(SensorFieldParseError::ToManyTokens);
    }

    let type_token = *tokens.first().ok_or(SensorFieldParseError::MissingType)?;
    let value_token = *tokens.get(1).ok_or(SensorFieldParseError::MissingValue)?;

    match type_token {
        "u" => {
            let value = value_token
                .parse()
                .map_err(|_| SensorFieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::UnsignedInt(value))
        }

        "i" => {
            let value = value_token
                .parse()
                .map_err(|_| SensorFieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::SignedInt(value))
        }

        "f" => {
            let value = value_token
                .parse()
                .map_err(|_| SensorFieldParseError::InvalidValue(value_token.to_string()))?;

            Ok(SensorValue::Float(value))
        }

        "b" => {
            let value = match value_token.to_lowercase().as_str() {
                "true" => true,
                "false" => false,
                _ => return Err(SensorFieldParseError::InvalidValue(value_token.to_string())),
            };

            Ok(SensorValue::Boolean(value))
        }

        _ => Err(SensorFieldParseError::InvalidType(type_token.to_string())),
    }
}
