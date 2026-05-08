use std::sync::Arc;

use andromeda_core::AndromedaResult;

use super::datum::Datum;
use super::encoder::RowEncoder;
use super::error::encoder_error;
use super::schema::{ColumnDef, RowSchema, ScalarType};

pub const INVENTORY_PRODUCT_STOCK_TABLE_NAME: &str = "Inventory.ProductStock";
pub const PRODUCT_STOCK_PRODUCT_ID_COLUMN: &str = "ProductId";
pub const PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN: &str = "QuantityOnHand";
pub const PRODUCT_STOCK_ROW_ENCODED_LEN: usize = 1 + 8 + 8;

/// Deterministic storage row shape for the minimal `Inventory.ProductStock`
/// heap-facing API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductStockRow {
    pub product_id: i64,
    pub quantity_on_hand: i64,
}

impl ProductStockRow {
    pub fn new(product_id: i64, quantity_on_hand: i64) -> AndromedaResult<Self> {
        if product_id <= 0 {
            return Err(encoder_error("ProductStock ProductId must be positive"));
        }
        if quantity_on_hand < 0 {
            return Err(encoder_error(
                "ProductStock QuantityOnHand must not be negative",
            ));
        }

        Ok(Self {
            product_id,
            quantity_on_hand,
        })
    }

    pub fn encode(self) -> AndromedaResult<Vec<u8>> {
        let encoder = product_stock_row_encoder()?;
        let encoded = encoder.encode(&self.to_datums())?;
        if encoded.len() != PRODUCT_STOCK_ROW_ENCODED_LEN {
            return Err(encoder_error(format!(
                "ProductStock row encoded length drift: expected {}, got {}",
                PRODUCT_STOCK_ROW_ENCODED_LEN,
                encoded.len()
            )));
        }
        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        if bytes.len() != PRODUCT_STOCK_ROW_ENCODED_LEN {
            return Err(encoder_error(format!(
                "ProductStock row encoded length mismatch: expected {}, got {}",
                PRODUCT_STOCK_ROW_ENCODED_LEN,
                bytes.len()
            )));
        }
        if bytes[0] != 0 {
            return Err(encoder_error(
                "ProductStock null bitmap and reserved bits must be zero",
            ));
        }
        let encoder = product_stock_row_encoder()?;
        let values = encoder.decode(bytes)?;
        Self::from_datums(&values)
    }

    pub fn to_datums(self) -> [Datum; 2] {
        [
            Datum::Int64(self.product_id),
            Datum::Int64(self.quantity_on_hand),
        ]
    }

    pub fn from_datums(values: &[Datum]) -> AndromedaResult<Self> {
        match values {
            [Datum::Int64(product_id), Datum::Int64(quantity_on_hand)] => {
                Self::new(*product_id, *quantity_on_hand)
            },
            _ => Err(encoder_error(
                "ProductStock row must contain ProductId:Int64 and QuantityOnHand:Int64",
            )),
        }
    }
}

pub fn product_stock_row_schema() -> AndromedaResult<Arc<RowSchema>> {
    Ok(Arc::new(RowSchema::new(vec![
        ColumnDef {
            name: PRODUCT_STOCK_PRODUCT_ID_COLUMN.to_string(),
            ordinal: 0,
            scalar_type: ScalarType::Int64,
            nullable: false,
        },
        ColumnDef {
            name: PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN.to_string(),
            ordinal: 1,
            scalar_type: ScalarType::Int64,
            nullable: false,
        },
    ])?))
}

pub fn product_stock_row_encoder() -> AndromedaResult<RowEncoder> {
    Ok(RowEncoder::new(product_stock_row_schema()?))
}
