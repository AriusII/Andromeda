use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub fn encode_protobuf_message<M>(message: &M) -> Vec<u8>
where
    M: prost::Message,
{
    message.encode_to_vec()
}

pub fn decode_protobuf_message<M>(bytes: &[u8], boundary_label: &str) -> AndromedaResult<M>
where
    M: prost::Message + Default,
{
    if bytes.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!("{boundary_label} decode failed: empty bytes are not valid protobuf"),
        ));
    }

    M::decode(bytes).map_err(|error| {
        AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!("{boundary_label} decode failed: {error}"),
        )
    })
}
