use andromeda_core::AndromedaResult;

use andromeda_wal::Lsn;

use super::error::storage_error;
use super::identity::PageId;
use super::layout::{PageLayoutContract, PageSize};

/// Full durable page byte image.
///
/// A `PageImage` is always exactly one canonical Andromeda page: either 16 KiB
/// or 32 KiB as selected by [`PageSize`]. The raw constructor intentionally
/// exposes no durable identity because binary header parsing is owned by the
/// page-layout contract. Callers that need `page_id` or `page_lsn` must attach a
/// validated [`PageLayoutContract`] with [`PageImage::with_layout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    page_size: PageSize,
    bytes: Box<[u8]>,
    layout_contract: Option<PageLayoutContract>,
}

impl PageImage {
    pub fn new(page_size: PageSize, bytes: Vec<u8>) -> AndromedaResult<Self> {
        Self::validate_len(page_size, bytes.len())?;
        Ok(Self {
            page_size,
            bytes: bytes.into_boxed_slice(),
            layout_contract: None,
        })
    }

    pub fn with_layout(
        layout_contract: PageLayoutContract,
        bytes: Vec<u8>,
    ) -> AndromedaResult<Self> {
        layout_contract.validate()?;
        let page_size = layout_contract.header.page_size;
        Self::validate_len(page_size, bytes.len())?;
        Ok(Self {
            page_size,
            bytes: bytes.into_boxed_slice(),
            layout_contract: Some(layout_contract),
        })
    }

    pub fn zeroed_with_layout(layout_contract: PageLayoutContract) -> AndromedaResult<Self> {
        layout_contract.validate()?;
        let bytes = vec![0; layout_contract.header.page_size.bytes_usize()];
        Self::with_layout(layout_contract, bytes)
    }

    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_vec()
    }

    pub fn layout_contract(&self) -> Option<PageLayoutContract> {
        self.layout_contract
    }

    pub fn page_id(&self) -> Option<PageId> {
        self.layout_contract.map(|contract| contract.header.page_id)
    }

    pub fn page_lsn(&self) -> Option<Lsn> {
        self.layout_contract
            .map(|contract| contract.header.page_lsn)
    }

    fn validate_len(page_size: PageSize, len: usize) -> AndromedaResult<()> {
        let expected = page_size.bytes_usize();
        if len != expected {
            return Err(storage_error(format!(
                "page image length {len} does not match canonical page size {expected}"
            )));
        }
        Ok(())
    }
}
