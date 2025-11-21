use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    time::{Duration, SystemTime},
};

use crate::serial::{SensorField, SensorValue};

/// A record of the stand's state saved to disk.
#[derive(Debug)]
pub struct StandRecord {
    file: File,
    field_names: Vec<String>,
    start_time: SystemTime,
}

impl StandRecord {
    /// Open a new [`StandRecord`] at the given path. Do not include the ".csv" file extension in
    /// the given path, it will be appended by this function.
    ///
    /// [`StandRecord`]: StandRecord
    pub fn open<P>(path: P, mut field_names: Vec<String>) -> io::Result<StandRecord>
    where
        P: AsRef<Path>,
    {
        let mut file = File::create(path.as_ref().with_extension("csv"))?;

        field_names.reverse();
        field_names.push("Time (Seconds)".to_string());
        field_names.reverse();

        let field_names_row = field_names
            .iter()
            .fold(String::new(), |acc, i| format!("{acc},{i}"));

        file.write_all(&mut field_names_row.into_bytes())?;
        file.flush()?;

        Ok(StandRecord {
            file,
            field_names,
            start_time: SystemTime::now(),
        })
    }

    /// Append the given [`SensorField`]s' values to the [`StandRecord`], timestamped with the
    /// current time since opening the [`StandRecord`]. Note that field who's names do not match
    /// those given in the [`StandRecord::open`] function will not be recorded.
    ///
    /// [`SensorField`]: SensorField
    /// [`StandRecord`]: StandRecord
    /// [`StandRecord::open`]: StandRecord::open
    pub fn append_frame(&mut self, fields: Vec<SensorField>) -> io::Result<()> {
        let now = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or(Duration::from_secs(0));

        let row = self
            .field_names
            .iter()
            .fold(format!("{}", now.as_secs_f64()), |acc, i| {
                let field = fields.iter().find(|f| f.name.as_str() == i.as_str());

                match field {
                    Some(f) => match f.value {
                        SensorValue::UnsignedInt(v) => format!("{acc},{v}"),
                        SensorValue::SignedInt(v) => format!("{acc},{v}"),
                        SensorValue::Float(v) => format!("{acc},{v}"),
                        SensorValue::Boolean(v) => format!("{acc},{v}"),
                    },

                    None => format!("{acc},"),
                }
            });

        self.file.write_all(&mut row.as_bytes())?;
        self.file.flush()?;

        Ok(())
    }
}
