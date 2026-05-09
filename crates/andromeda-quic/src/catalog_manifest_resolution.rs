//! QUIC route contract for catalog procedure manifest resolution.
//!
//! Catalog manifest resolution is transported as a reliable command stream:
//! `ContractRequest` frames carry a protobuf `FrameEnvelope` whose payload is a
//! generated `CatalogProcedureManifestResolutionRequest`; route responses use
//! `ContractResponse` with a generated
//! `CatalogProcedureManifestResolutionResponse`.

mod errors;
mod gateway;

pub use gateway::{
    CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRuntime,
};
