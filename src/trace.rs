use std::io;

macro_rules! trace {
    (track_caller: $($tt:tt)*) => {
        tracing::trace!(
            location = %{
                let location = std::panic::Location::caller();
                format_args!("{}:{}:{}", location.file(), location.line(), location.column())
            },
            $($tt)*
        )
    };
    ($($tt:tt)*) => {
        tracing::trace!(
            location = %format_args!("{}:{}:{}", file!(), line!(), column!()),
            $($tt)*
        )
    };
}

macro_rules! debug {
    (track_caller: $($tt:tt)*) => {
        tracing::debug!(
            location = %{
                let location = std::panic::Location::caller();
                format_args!("{}:{}:{}", location.file(), location.line(), location.column())
            },
            $($tt)*
        )
    };
    ($($tt:tt)*) => {
        tracing::debug!(
            location = %format_args!("{}:{}:{}", file!(), line!(), column!()),
            $($tt)*
        )
    };
}

pub(crate) fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .init();
}
