use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::ContractHash;

pub fn stable_contract_hash(domain: &[u8], bytes: &[u8]) -> ContractHash {
    ContractHash::new(stable_hash_256(domain, bytes))
}

pub fn stable_hash_256(domain: &[u8], bytes: &[u8]) -> [u8; ContractHash::LEN] {
    let mut output = [0_u8; ContractHash::LEN];
    let seeds = [
        0xcbf2_9ce4_8422_2325_u64,
        0x8422_2325_cbf2_9ce4_u64,
        0x9e37_79b9_7f4a_7c15_u64,
        0x94d0_49bb_1331_11eb_u64,
    ];

    for (index, seed) in seeds.into_iter().enumerate() {
        let hash = stable_hash64(seed, domain, bytes);
        let start = index * 8;
        output[start..start + 8].copy_from_slice(&hash.to_be_bytes());
    }

    output
}

pub fn validate_optional_contract_hash(label: &str, bytes: Option<&[u8]>) -> AndromedaResult<()> {
    if let Some(bytes) = bytes {
        validate_required_contract_hash(label, bytes)?;
    }

    Ok(())
}

pub fn validate_required_contract_hash(label: &str, bytes: &[u8]) -> AndromedaResult<()> {
    if bytes.len() != ContractHash::LEN {
        return contract_error(format!("{label} must be 32 bytes"));
    }

    if bytes.iter().all(|byte| *byte == 0) {
        return contract_error(format!("{label} must not be zero"));
    }

    Ok(())
}

fn stable_hash64(seed: u64, domain: &[u8], bytes: &[u8]) -> u64 {
    let mut hash = seed ^ ((domain.len() as u64) << 32) ^ bytes.len() as u64;

    for byte in domain {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(17);
    }

    hash ^= 0xff;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(31);
    }

    hash
}

fn contract_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        message.into(),
    ))
}
