use std::time::SystemTime;

use hearth_runtime::{
    async_trait,
    flue::Table,
    hearth_macros::GetProcessMetadata,
    runtime::{Plugin, RuntimeBuilder},
    tokio::{
        self,
        time::{Duration, Instant},
    },
    tracing::debug,
    utils::{
        MessageInfo, RequestInfo, RequestResponseProcess, ResponseInfo, RunnerContext,
        ServiceRunner, SinkProcess,
    },
};

/// A plugin that provides timing services to guests.
///
/// Adds the following services:
/// - [SleepService]
/// - [TimerFactory]
/// - [StopwatchFactory]
/// - [UnixTimeService]
#[derive(Default)]
pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        builder
            .add_plugin(SleepService)
            .add_plugin(TimerFactory)
            .add_plugin(StopwatchFactory)
            .add_plugin(UnixTimeService);
    }
}

/// Receives a single floating-point number as a request, waits the value of
/// the number in seconds, then responds with an empty message.
#[derive(GetProcessMetadata)]
pub struct SleepService;

// This cannot be a [RequestResponse] type because the response must be sent
// asynchronously.
#[async_trait]
impl SinkProcess for SleepService {
    type Message = f32;

    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>) {
        let Some(reply) = message.caps.first() else {
            debug!("Sleep request has no reply address");
            return;
        };

        let duration = Duration::from_secs_f32(message.data);
        let reply = reply.to_owned();
        let post = message.runtime.post.to_owned();

        // spawn thread to reply so that we can receive more sleep requests
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;

            let table = Table::new(post);
            let reply_handle = table.import_owned(reply).unwrap();
            let reply_cap = table.wrap_handle(reply_handle).unwrap();
            reply_cap.send(&[], &[]).await.unwrap();
        });
    }
}

impl ServiceRunner for SleepService {
    const NAME: &'static str = "hearth.Sleep";
}

/// Responds to empty request messages with a capability to a new instance of
/// a [Timer].
#[derive(GetProcessMetadata)]
pub struct TimerFactory;

#[async_trait]
impl RequestResponseProcess for TimerFactory {
    type Request = ();
    type Response = ();

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let last_request = Instant::now();
        let timer = Timer { last_request };
        let child = request.spawn(timer);

        ResponseInfo {
            data: (),
            caps: vec![child],
        }
    }
}

impl ServiceRunner for TimerFactory {
    const NAME: &'static str = "hearth.TimerFactory";
}

/// Waits a given interval beginning precisely from the end of the last wait.
///
/// Receives a floating-point number as a request, waits that number in seconds,
/// then sends back an empty message as response.
///
/// A timer's wait begins at the end of its last wait as opposed to when it
/// recievives a request. This is an improvement over [SleepService] as timers
/// function entirely on their internal clock and thus eliminate potential
/// round-trip time between requests and responses accumulating over multiple
/// sleep requests.
#[derive(GetProcessMetadata)]
pub struct Timer {
    last_request: Instant,
}

#[async_trait]
impl RequestResponseProcess for Timer {
    type Request = f32;
    type Response = ();

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let duration = Duration::from_secs_f32(request.data);
        self.last_request += duration;
        tokio::time::sleep_until(self.last_request).await;

        ResponseInfo {
            data: (),
            caps: vec![],
        }
    }
}

/// Responds to empty request messages with a capability to a new instance of
/// a [Stopwatch].
#[derive(GetProcessMetadata)]
pub struct StopwatchFactory;

#[async_trait]
impl RequestResponseProcess for StopwatchFactory {
    type Request = ();
    type Response = ();

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let last_request = Instant::now();
        let timer = Stopwatch { last_request };
        let child = request.spawn(timer);

        ResponseInfo {
            data: (),
            caps: vec![child],
        }
    }
}

impl ServiceRunner for StopwatchFactory {
    const NAME: &'static str = "hearth.StopwatchFactory";
}

/// Responds to empty request messages with a floating-point number that
/// represents the time in seconds since the last request.
#[derive(GetProcessMetadata)]
pub struct Stopwatch {
    last_request: Instant,
}

#[async_trait]
impl RequestResponseProcess for Stopwatch {
    type Request = ();
    type Response = f32;

    async fn on_request<'a>(
        &'a mut self,
        _request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_request).as_secs_f32();
        self.last_request = now;

        ResponseInfo {
            data: elapsed,
            caps: vec![],
        }
    }
}

/// Native service that returns time since the UNIX epoch in nanoseconds as an
/// unsigned 128-bit integer.
#[derive(GetProcessMetadata)]
pub struct UnixTimeService;

#[async_trait]
impl RequestResponseProcess for UnixTimeService {
    type Request = ();
    type Response = u128;

    async fn on_request<'a>(
        &'a mut self,
        _request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let time_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time before UNIX epoch");

        ResponseInfo {
            data: time_since_epoch.as_nanos(),
            caps: vec![],
        }
    }
}

impl ServiceRunner for UnixTimeService {
    const NAME: &'static str = "hearth.UnixTime";
}
