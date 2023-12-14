#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        ::hearth_guest::log(
            $level,
            ::core::module_path!(),
            &format!($($arg)*),
        )
    }
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Trace, $($arg)*);
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Debug, $($arg)*);
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Info, $($arg)*);
    };
}

#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Warning, $($arg)*);
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Error, $($arg)*);
    };
}

pub use debug;
pub use error;
pub use info;
pub use log;
pub use trace;
pub use warning;
