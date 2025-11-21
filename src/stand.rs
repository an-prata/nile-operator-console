use crate::serial::{self, SensorField, SensorValue};
use std::{error::Error, fmt::Display};

/// Structure representing the state of the NILE stand.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Default)]
pub struct StandState {
    stand_mode: StandMode,

    pub valve_np1: Option<ValveState>,
    pub valve_np2: Option<ValveState>,
    pub valve_np3: Option<ValveState>,
    pub valve_np4: Option<ValveState>,

    pub valve_ip1: Option<ValveState>,
    pub valve_ip2: Option<ValveState>,
    pub valve_ip3: Option<ValveState>,
}

/// State of a single valve.
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ValveState {
    Open,
    Closed,
}

impl StandState {
    pub fn transition_mode(&mut self, mode: StandMode) -> Result<(), ModeTransitionError> {
        mode.check_transition(self)?;
        self.stand_mode = mode;
        Ok(())
    }

    pub fn update(&mut self, fields: &[SensorField]) {
        self.valve_np1 = valve_state("NP1_OPEN", &fields);
        self.valve_np2 = valve_state("NP2_OPEN", &fields);
        self.valve_np3 = valve_state("NP3_OPEN", &fields);
        self.valve_np4 = valve_state("NP4_OPEN", &fields);
        self.valve_ip1 = valve_state("IP1_OPEN", &fields);
        self.valve_ip2 = valve_state("IP2_OPEN", &fields);
        self.valve_ip3 = valve_state("IP3_OPEN", &fields);
    }

    pub fn mode(&self) -> StandMode {
        self.stand_mode
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

/// The different modes that the NILE stand software can take on.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub enum StandMode {
    /// Complete manual control of valves.
    CheckOut,

    /// Limits control to control of valves [`serial::NILE_VALVE_NP3`] and
    /// [`serial::NILE_VALVE_NP4`].
    ///
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_NP4`]: serial::NILE_VALVE_NP4
    OxygenFilling,

    /// Manual control over valves [`serial::NILE_VALVE_NP2`], [`serial::NILE_VALVE_IP2`],
    /// [`serial::NILE_VALVE_NP3`], and [`serial::NILE_VALVE_IP3`]. Ability to begin sequence which
    /// ingnites the ignitor, then opens [`serial::NILE_VALVE_NP1`] and [`serial::NILE_VALVE_IP1`]
    /// simultaniously. Operators can enter a firing time which holds [`serial::NILE_VALVE_NP1`] and
    /// [`serial::NILE_VALVE_IP1`] open for that time plus three seconds to clear excess propellant.
    /// After this time plus three seconds [`serial::NILE_VALVE_NP1`], [`serial::NILE_VALVE_IP1`],
    /// [`serial::NILE_VALVE_NP2`], and [`serial::NILE_VALVE_IP2`] will all close while
    /// [`serial::NILE_VALVE_NP3`] and [`serial::NILE_VALVE_IP3`] open to vent excess nitrogen
    /// - "Fire".
    ///
    /// NOTE: Maybe have entry for timing delays between NP1 and IP1, though this is probably best
    /// done on the stand side.
    ///
    /// [`serial::NILE_VALVE_NP1`]: serial::NILE_VALVE_NP1
    /// [`serial::NILE_VALVE_IP1`]: serial::NILE_VALVE_IP1
    /// [`serial::NILE_VALVE_NP2`]: serial::NILE_VALVE_NP2
    /// [`serial::NILE_VALVE_IP2`]: serial::NILE_VALVE_IP2
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_IP3`]: serial::NILE_VALVE_IP3
    PressurizationAndFiring,

    /// Sets [`serial::NILE_VALVE_NP3`] and [`serial::NILE_VALVE_IP3`] open and closes all others.
    /// Also allows for operators to use a "Depress System" button which will open
    /// [`serial::NILE_VALVE_NP4`] for five seconds then closes it, followed by opening
    /// [`serial::NILE_VALVE_IP2`] for five seconds then closing it, followed by finally opening
    /// [`serial::NILE_VALVE_NP2`] for five seconds and then closing it, there should be one second
    /// delay between all valve openings - "Depressurize System".
    ///
    /// [`serial::NILE_VALVE_NP2`]: serial::NILE_VALVE_NP2
    /// [`serial::NILE_VALVE_IP2`]: serial::NILE_VALVE_IP2
    /// [`serial::NILE_VALVE_NP3`]: serial::NILE_VALVE_NP3
    /// [`serial::NILE_VALVE_IP3`]: serial::NILE_VALVE_IP3
    /// [`serial::NILE_VALVE_NP4`]: serial::NILE_VALVE_NP4
    #[default]
    Safing,
}

impl StandMode {
    /// Returns a [`Vec`] of the valves which may be manually controlled in the given [`StandMode`].
    ///
    /// [`Vec`]: Vec
    /// [`StandMode`]: StandMode
    pub fn manual_control_valves(self) -> Vec<&'static str> {
        match self {
            Self::CheckOut => vec![
                serial::NILE_VALVE_NP1,
                serial::NILE_VALVE_NP2,
                serial::NILE_VALVE_NP3,
                serial::NILE_VALVE_NP4,
                serial::NILE_VALVE_IP1,
                serial::NILE_VALVE_IP2,
                serial::NILE_VALVE_IP3,
            ],

            Self::OxygenFilling => vec![serial::NILE_VALVE_NP3, serial::NILE_VALVE_NP4],

            Self::PressurizationAndFiring => vec![
                serial::NILE_VALVE_NP2,
                serial::NILE_VALVE_NP3,
                serial::NILE_VALVE_IP2,
                serial::NILE_VALVE_IP3,
            ],

            Self::Safing => vec![],
        }
    }

    /// Check the necessary conditions for moving out of the current [`StandMode`] and into the
    /// desired [`StandMode`] against the current [`StandState`].
    ///
    /// [`StandMode`]: StandMode
    /// [`StandState`]: StandState
    fn check_transition(&self, state: &StandState) -> Result<(), ModeTransitionError> {
        // Checks for moving _out_ of a state.
        match state.stand_mode {
            StandMode::CheckOut => (),
            StandMode::OxygenFilling => check_pretransition_ox_filling(state)?,
            StandMode::PressurizationAndFiring => (),
            StandMode::Safing => (),
        }

        // Checks for moving _into_ a given state.
        match self {
            StandMode::CheckOut => Ok(()),
            StandMode::OxygenFilling => check_transition_ox_filling(state),
            StandMode::PressurizationAndFiring => Ok(()),
            StandMode::Safing => Ok(()),
        }
    }
}

impl Display for StandMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StandMode::CheckOut => write!(f, "Check Out Mode"),
            StandMode::OxygenFilling => write!(f, "Ox Filling Mode"),
            StandMode::PressurizationAndFiring => write!(f, "Pressurization & Firing Mode"),
            StandMode::Safing => write!(f, "Safing Mode"),
        }
    }
}

impl Into<String> for StandMode {
    fn into(self) -> String {
        format!("{}", self)
    }
}

/// Produce and error if a transition out of [`OxygenFilling`] mode would be erronious with the
/// given [`StandState`].
///
/// [`OxygenFilling`]: StandMode::OxygenFilling
/// [`StandState`]: StandState
fn check_pretransition_ox_filling(state: &StandState) -> Result<(), ModeTransitionError> {
    match state {
        StandState {
            valve_np3: Some(ValveState::Closed),
            valve_np4: Some(ValveState::Closed),
            ..
        } => Ok(()),

        _ => Err(ModeTransitionError(
            "Expected valves NP3 and NP4 to be closed",
        )),
    }
}

/// Produce and error if a transition into [`OxygenFilling`] would be erronious with the given
/// [`StandState`].
///
/// [`OxygenFilling`]: StandMode::OxygenFilling
/// [`StandState`]: StandState
fn check_transition_ox_filling(state: &StandState) -> Result<(), ModeTransitionError> {
    match state {
        StandState {
            valve_np1: Some(ValveState::Closed),
            valve_np2: Some(ValveState::Closed),
            valve_np3: Some(ValveState::Closed),
            valve_np4: Some(ValveState::Closed),

            valve_ip1: Some(ValveState::Closed),
            valve_ip2: Some(ValveState::Closed),
            valve_ip3: Some(ValveState::Closed),
            ..
        } => Ok(()),

        _ => Err(ModeTransitionError("Expected all valves to be closed")),
    }
}

/// Failures for transitioning between [`StandMode`]s.
///
/// [`StandMode`]: StandMode
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ModeTransitionError(&'static str);

impl Display for ModeTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error transitioning states: {}", self.0)
    }
}

impl Error for ModeTransitionError {}
