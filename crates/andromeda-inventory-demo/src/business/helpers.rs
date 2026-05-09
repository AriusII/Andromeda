use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

pub(super) fn validate_business_product_id(product_id: i64) -> AndromedaResult<()> {
    if product_id <= 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "inventory product id must be positive",
        ));
    }

    Ok(())
}

pub(super) fn validate_business_mvcc_timestamp(
    timestamp: u64,
    message: &'static str,
) -> AndromedaResult<()> {
    if timestamp == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

pub(super) fn validate_business_mvcc_transaction_id(
    transaction_id: TransactionId,
    message: &'static str,
) -> AndromedaResult<()> {
    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}
