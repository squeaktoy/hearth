use super::*;

lazy_static::lazy_static! {
    static ref SLEEP_SERVICE: Capability =
        registry::REGISTRY.get_service("hearth.Sleep")
            .expect("requested service \"hearth.Sleep\" is unavailable");

    static ref TIMER_FACTORY: RequestResponse<(), ()> =
        RequestResponse::expect_service("hearth.TimerFactory");

    static ref STOPWATCH_FACTORY: RequestResponse<(), ()> =
        RequestResponse::expect_service("hearth.StopwatchFactory");

    static ref UNIX_TIME: RequestResponse<(), u128> =
        RequestResponse::expect_service("hearth.UnixTime");
}

/// Sleeps for the given time in seconds.
pub fn sleep(duration: f32) {
    let reply = Mailbox::new();
    let reply_cap = reply.make_capability(Permissions::SEND);
    reply.monitor(&SLEEP_SERVICE);

    SLEEP_SERVICE.send(&duration, &[&reply_cap]);

    let _ = reply.recv_raw();
}

/// Gets the time since the UNIX epoch in nanoseconds as a unsigned 128-bit
/// integer.
pub fn get_unix_time() -> u128 {
    UNIX_TIME.request((), &[]).0
}

pub struct Timer(RequestResponse<f32, ()>);

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    /// Creates a new Timer.
    pub fn new() -> Self {
        let (_, resp) = TIMER_FACTORY.request((), &[]);
        Self(RequestResponse::new(resp.get(0).unwrap().clone()))
    }

    /// Sleeps the given time in seconds from the end of the last tick.
    pub fn tick(&self, duration: f32) {
        self.0.request(duration, &[]);
    }
}

pub struct Stopwatch(RequestResponse<(), f32>);

impl Default for Stopwatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Stopwatch {
    /// Creates a new Stopwatch.
    pub fn new() -> Self {
        let (_, resp) = STOPWATCH_FACTORY.request((), &[]);
        Self(RequestResponse::new(resp.get(0).unwrap().clone()))
    }

    /// Responds with the time since the last request.
    pub fn lap(&self) -> f32 {
        self.0.request((), &[]).0
    }
}
