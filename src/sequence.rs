use std::{
    fmt::Display,
    sync::mpsc::{SendError, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug, Default)]
pub struct CommandSequence {
    commands: Vec<Command>,
}

impl CommandSequence {
    /// Create a new, empty [`CommandSequence`].
    ///
    /// [`CommandSequence`]: CommandSequence
    pub fn new() -> Self {
        CommandSequence {
            commands: Vec::new(),
        }
    }

    /// Append a [`Command`] to the [`CommandSequence`] and return it.
    ///
    /// [`Command`]: Command
    /// [`CommandSequence`]: CommandSequence
    pub fn then(mut self, command: Command) -> CommandSequence {
        self.commands.push(command);
        self
    }

    /// Run the [`CommandSequence`] by running each of its [`Command`]s.
    ///
    /// [`CommandSequence`]: CommandSequence
    /// [`Command`]: Command
    pub fn run(self, mut tx: Sender<Vec<u8>>) -> Result<(), SendError<Vec<u8>>> {
        for command in self.commands {
            command.run(&mut tx)?
        }

        Ok(())
    }

    /// Run the [`CommandSequence`] by running each of its [`Command`]s in order in a new thread.
    ///
    /// [`CommandSequence`]: CommandSequence
    /// [`Command`]: Command
    pub fn run_par(self, tx: Sender<Vec<u8>>) -> JoinHandle<Result<(), SendError<Vec<u8>>>> {
        thread::spawn(move || self.run(tx))
    }
}

#[derive(Debug)]
pub enum Command {
    OpenValve(ValveHandle),
    CloseValve(ValveHandle),
    Ignite,
    Safe,
    Fire,
    Wait(Duration),
}

impl Command {
    /// Run the given [`Command`], sending thm to the given [`Sender`].
    ///
    /// [`Command`]: Command
    /// [`Sender`]: Sender
    fn run(self, tx: &mut Sender<Vec<u8>>) -> Result<(), SendError<Vec<u8>>> {
        match self {
            Command::OpenValve(valve_handle) => {
                tx.send(format!("OPEN:{valve_handle}\n").into_bytes())
            }

            Command::CloseValve(valve_handle) => {
                tx.send(format!("CLOSE:{valve_handle}\n").into_bytes())
            }

            Command::Ignite => tx.send("IGNITE\n".to_string().into_bytes()),
            Command::Safe => tx.send("SAFE\n".to_string().into_bytes()),
            Command::Fire => tx.send("FIRE\n".to_string().into_bytes()),

            Command::Wait(duration) => {
                thread::sleep(duration);
                Ok(())
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ValveHandle {
    NP1,
    NP2,
    NP3,
    NP4,
    IP1,
    IP2,
    IP3,
}

impl Display for ValveHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValveHandle::NP1 => write!(f, "NP1"),
            ValveHandle::NP2 => write!(f, "NP2"),
            ValveHandle::NP3 => write!(f, "NP3"),
            ValveHandle::NP4 => write!(f, "NP4"),
            ValveHandle::IP1 => write!(f, "IP1"),
            ValveHandle::IP2 => write!(f, "IP2"),
            ValveHandle::IP3 => write!(f, "IP3"),
        }
    }
}
