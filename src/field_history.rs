use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct ValueHistory<T>
where
    T: Clone + PartialEq,
{
    history: Vec<HistoricalValue<T>>,
}

impl<T> ValueHistory<T>
where
    T: Clone + PartialEq,
{
    /// Creates a new empty [`FieldHistory`].
    ///
    /// [`FieldHistory`]: FieldHistory
    pub fn new() -> Self {
        ValueHistory {
            history: Vec::new(),
        }
    }

    pub fn top(&self) -> Option<&T> {
        self.history.last().map(|hist| &hist.value)
    }

    /// Pushes a new value onto the [`FieldHistory`], stamping it with the current time.
    ///
    /// [`FieldHistory`]: FieldHistory
    pub fn push(&mut self, value: T) {
        self.history.push(HistoricalValue::from_now(value));
    }

    pub fn as_point_span(&self, span: Duration) -> Vec<(Duration, T)> {
        let now = SystemTime::now();

        self.history
            .iter()
            .map(|HistoricalValue { value, time }| {
                (
                    now.duration_since(*time)
                        .expect("`time` will always be earlier than `now`"),
                    value.to_owned(),
                )
            })
            .filter(|&(t, _)| t <= span)
            .collect()
    }

    /// Prune the [`ValueHistory`] to include only historical values form within the given
    /// [`Duration`] to now.
    ///
    /// [`ValueHistory`]: ValueHistory
    /// [`Duration`]: Duration
    pub fn prune(&mut self, span: Duration) {
        let now = SystemTime::now();

        self.history = self
            .history
            .iter()
            .filter(|value| {
                now.duration_since(value.time)
                    .expect("`time` will always be earlier than `now`")
                    <= span
            })
            .map(|x| x.to_owned())
            .collect();
    }
}

#[derive(Debug, Clone)]
pub struct HistoricalValue<T> {
    value: T,
    time: SystemTime,
}

impl<T> HistoricalValue<T> {
    pub fn from_now(value: T) -> Self {
        HistoricalValue {
            value,
            time: SystemTime::now(),
        }
    }
}
