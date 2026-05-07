//! QUIC route contract for catalog procedure manifest resolution.
//!
//! Catalog manifest resolution is transported as a reliable command stream:
//! `ContractRequest` frames carry a protobuf `FrameEnvelope` whose payload is a
//! generated `CatalogProcedureManifestResolutionRequest`; route responses use
//! `ContractResponse` with a generated
//! `CatalogProcedureManifestResolutionResponse`.

use andromeda_proto::generated;

mod errors;
mod frame;
mod gateway;
mod manifest;
mod validation;

pub use generated::contract::v1::{
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    catalog_procedure_manifest_resolution_response::Status as CatalogManifestResolutionStatus,
};

type GeneratedCatalogManifestResolutionRequest =
    generated::contract::v1::CatalogProcedureManifestResolutionRequest;
type GeneratedCatalogManifestResolutionResponse =
    generated::contract::v1::CatalogProcedureManifestResolutionResponse;
type GeneratedCatalogManifestSelector =
    generated::contract::v1::catalog_procedure_manifest_resolution_request::Selector;
type GeneratedProcedureManifest = generated::contract::v1::ProcedureManifest;
type GeneratedProtocolLayout = generated::contract::v1::ProtocolLayout;
type GeneratedRequiredPermission = generated::contract::v1::RequiredPermission;
type GeneratedResultStreamDescriptor = generated::contract::v1::ResultStreamDescriptor;
type GeneratedColumnDescriptor = generated::contract::v1::ColumnDescriptor;
type GeneratedFrameEnvelope = generated::protocol::v1::FrameEnvelope;
type GeneratedProtocolVersion = generated::protocol::v1::ProtocolVersion;

pub use frame::{
    catalog_manifest_resolution_request_frame, decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
};
pub use gateway::{
    CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRuntime,
};
pub use manifest::{
    CatalogColumnDescriptor, CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestSelector, CatalogProcedureManifest, CatalogProcedureProtocolLayout,
    CatalogRequiredPermission, CatalogResultStreamDescriptor,
};
