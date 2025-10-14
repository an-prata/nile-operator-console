use crate::serial::{SensorField, SensorValue};

/// Structure representing the state of the NILE stand.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Default)]
pub struct StandState {
    pub valve_np1: Option<ValveState>,
    pub valve_np2: Option<ValveState>,
    pub valve_np3: Option<ValveState>,
    pub valve_np4: Option<ValveState>,

    pub valve_ip1: Option<ValveState>,
    pub valve_ip2: Option<ValveState>,
    pub valve_ip3: Option<ValveState>,
    // TODO: Add non "boolean" fields.
}

/// State of a single valve.
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ValveState {
    Open,
    Closed,
}

impl StandState {
    /// Creates a new [`StandState`] from the given [`SensorField`]s. If a [`StandState`] field's
    /// corrosponding [`SensorField`] is not present then it is assigned [`None`].
    ///
    /// [`StandState`]: StandState
    /// [`SensorField`]: SensorField
    /// [`None`]: Option::None
    pub fn from_fields(fields: &[SensorField]) -> StandState {
        StandState {
            valve_np1: valve_state("NP1_OPEN", &fields),
            valve_np2: valve_state("NP2_OPEN", &fields),
            valve_np3: valve_state("NP3_OPEN", &fields),
            valve_np4: valve_state("NP4_OPEN", &fields),
            valve_ip1: valve_state("IP1_OPEN", &fields),
            valve_ip2: valve_state("IP2_OPEN", &fields),
            valve_ip3: valve_state("IP3_OPEN", &fields),
        }
    }
}

/// Checks for a [`SensorField`] with the given name, if it exists and its value is
/// [`SensorValue::Boolean(true)`] this function returns [`ValveState::Open`], if its value is
/// [`SensorValue::Boolean(false)`] then [`ValveState::Closed`] is returned. If the field does not
/// exist, or is not a [`SensorValue::Boolean`], [`None`] is returned.
///
/// [`None`]: Option::None
/// [`SensorField`]: SensorField
/// [`SensorValue::Boolean(true)`]: SensorValue::Boolean
/// [`SensorValue::Boolean(false)`]: SensorValue::Boolean
/// [`SensorValue::Boolean`]: SensorValue::Boolean
/// [`ValveState::Closed`]: ValveState::Closed
fn valve_state(name: &str, fields: &[SensorField]) -> Option<ValveState> {
    fields
        .iter()
        .find(|field| field.name.as_str() == name)
        .and_then(|f| match f.value {
            SensorValue::Boolean(true) => Some(ValveState::Open),
            SensorValue::Boolean(false) => Some(ValveState::Closed),
            _ => None,
        })
}
