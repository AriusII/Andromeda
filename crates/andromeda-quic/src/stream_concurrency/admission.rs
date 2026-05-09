use andromeda_error::AndromedaResult;
use andromeda_types::InvocationId;
use std::collections::{HashMap, hash_map::Entry};
use std::time::SystemTime;

use super::errors;
use super::limits::StreamConcurrencyLimits;
use super::state::{CancellationToken, StreamMetadata};

pub(super) fn admit_stream(
    streams: &mut HashMap<InvocationId, StreamMetadata>,
    limits: StreamConcurrencyLimits,
    invocation_id: InvocationId,
    now: SystemTime,
) -> AndromedaResult<CancellationToken> {
    if streams.len() >= limits.max_concurrent() {
        return Err(errors::stream_limit_reached());
    }

    let metadata = StreamMetadata::new(invocation_id, now);
    let cancellation_token = metadata.cancellation_token();

    match streams.entry(invocation_id) {
        Entry::Occupied(_) => Err(errors::duplicate_stream()),
        Entry::Vacant(entry) => {
            entry.insert(metadata);
            Ok(cancellation_token)
        },
    }
}
