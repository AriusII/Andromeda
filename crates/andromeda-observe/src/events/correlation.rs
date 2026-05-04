use andromeda_core::{
    CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCorrelation {
    pub protocol_version: Option<u16>,
    pub stream_id: Option<u64>,
    pub stream_role: Option<u16>,
    pub frame_type: Option<u16>,
    pub payload_kind: Option<u16>,
    pub sequence: Option<u64>,
}

impl ProtocolCorrelation {
    pub const fn empty() -> Self {
        Self {
            protocol_version: None,
            stream_id: None,
            stream_role: None,
            frame_type: None,
            payload_kind: None,
            sequence: None,
        }
    }

    pub const fn has_frame_evidence(self) -> bool {
        self.stream_id.is_some() && self.frame_type.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolEventScope {
    Connection,
    Session,
    Request,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventCorrelation {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub contract_hash: Option<ContractHash>,
    pub catalog_version: Option<CatalogVersion>,
    pub catalog_object_id: Option<CatalogObjectId>,
    pub transaction_id: Option<TransactionId>,
    pub durable_lsn: Option<u64>,
    pub protocol: Option<ProtocolCorrelation>,
}

impl EventCorrelation {
    pub const fn empty() -> Self {
        Self {
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
            protocol: None,
        }
    }

    pub fn has_request_session(self) -> bool {
        self.request_id
            .is_some_and(|request_id| request_id.get() != 0)
            && self
                .session_id
                .is_some_and(|session_id| session_id.get() != 0)
    }

    pub fn has_contract_catalog(self) -> bool {
        self.contract_hash
            .is_some_and(|contract_hash| !contract_hash.is_zero())
            && self
                .catalog_version
                .is_some_and(|catalog_version| catalog_version.get() != 0)
    }

    pub fn has_no_transaction_evidence(self) -> bool {
        self.transaction_id.is_none() && self.durable_lsn.is_none()
    }

    pub fn has_transaction_evidence(self) -> bool {
        self.transaction_id
            .is_some_and(|transaction_id| transaction_id.get() != 0)
    }

    pub fn has_durable_lsn(self) -> bool {
        self.durable_lsn.is_some_and(|durable_lsn| durable_lsn != 0)
    }
}
