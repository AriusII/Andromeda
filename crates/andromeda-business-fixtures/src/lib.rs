#![forbid(unsafe_code)]

//! Shared business-domain fixtures for tests and examples.

use andromeda_storage_page::{PageId, PageSize};

pub mod product_stock {
    use super::*;

    pub const DEFAULT_PRODUCT_ID: i64 = 42;
    pub const DEFAULT_AVAILABLE_QUANTITY: i64 = 10;
    pub const DEFAULT_STOCK_VERSION: u64 = 7;
    pub const DEFAULT_RESERVE_QUANTITY: i64 = 3;
    pub const DEFAULT_PAGE_ID: u64 = 31_001;
    pub const DEFAULT_PAGE_SIZE: PageSize = PageSize::KiB16;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProductStockFixture {
        pub product_id: i64,
        pub available_quantity: i64,
        pub stock_version: u64,
        pub reserve_quantity: i64,
        pub page_id: u64,
        pub page_size: PageSize,
    }

    impl ProductStockFixture {
        pub const fn default_inventory_reserve_stock() -> Self {
            Self {
                product_id: DEFAULT_PRODUCT_ID,
                available_quantity: DEFAULT_AVAILABLE_QUANTITY,
                stock_version: DEFAULT_STOCK_VERSION,
                reserve_quantity: DEFAULT_RESERVE_QUANTITY,
                page_id: DEFAULT_PAGE_ID,
                page_size: DEFAULT_PAGE_SIZE,
            }
        }

        pub const fn storage_page_id(self) -> PageId {
            PageId::new(self.page_id)
        }
    }

    pub const fn default_fixture() -> ProductStockFixture {
        ProductStockFixture::default_inventory_reserve_stock()
    }

    pub const fn default_stock_tuple() -> (i64, i64, u64) {
        (
            DEFAULT_PRODUCT_ID,
            DEFAULT_AVAILABLE_QUANTITY,
            DEFAULT_STOCK_VERSION,
        )
    }

    pub const fn default_reserve_command_tuple() -> (i64, i64) {
        (DEFAULT_PRODUCT_ID, DEFAULT_RESERVE_QUANTITY)
    }
}

pub use product_stock::{
    DEFAULT_AVAILABLE_QUANTITY, DEFAULT_PAGE_ID, DEFAULT_PAGE_SIZE, DEFAULT_PRODUCT_ID,
    DEFAULT_RESERVE_QUANTITY, DEFAULT_STOCK_VERSION, ProductStockFixture, default_fixture,
    default_reserve_command_tuple, default_stock_tuple,
};
