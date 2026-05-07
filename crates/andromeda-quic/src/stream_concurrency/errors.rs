use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn stream_limit_reached() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Resource,
        "stream concurrency limit reached",
    )
}

pub(super) fn duplicate_stream() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transport,
        "stream already exists for this invocation ID",
    )
}

pub(super) fn invalid_stream_state(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transport, message)
}

pub(super) fn stream_not_found() -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transport, "stream not found")
}
