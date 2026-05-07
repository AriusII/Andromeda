use andromeda_core::{
    AndromedaError, AndromedaResult, CatalogVersion, ContractHash, ProcedureId, RequestId,
};
use andromeda_proto::{
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};

use super::errors::{contract_error, protocol_error};
use super::{
    CatalogManifestResolutionStatus, GeneratedCatalogManifestResolutionRequest,
    GeneratedCatalogManifestResolutionResponse, GeneratedCatalogManifestSelector,
    GeneratedColumnDescriptor, GeneratedProcedureManifest, GeneratedProtocolLayout,
    GeneratedRequiredPermission, GeneratedResultStreamDescriptor,
};

/// QUIC-local domain selector for catalog manifest resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestSelector {
    ProcedureId(ProcedureId),
    ProcedureName(String),
}

impl CatalogManifestSelector {
    fn validate_resolved_manifest(
        &self,
        manifest: &CatalogProcedureManifest,
    ) -> AndromedaResult<()> {
        match self {
            CatalogManifestSelector::ProcedureId(expected) => {
                if manifest.procedure_id != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure id does not match request selector",
                    ));
                }
            }
            CatalogManifestSelector::ProcedureName(expected) => {
                if manifest.procedure_name != *expected {
                    return Err(contract_error(
                        "resolved catalog manifest procedure name does not match request selector",
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Domain request used inside the QUIC route after protobuf boundary decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionRequest {
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub request_id: RequestId,
    pub trace_id: Option<String>,
    pub selector: CatalogManifestSelector,
    pub expected_contract_hash: Option<ContractHash>,
    pub expected_catalog_version: Option<CatalogVersion>,
    pub require_source_generator_ready: bool,
}

impl CatalogManifestResolutionRequest {
    pub fn from_protobuf(
        request: GeneratedCatalogManifestResolutionRequest,
    ) -> AndromedaResult<Self> {
        validate_catalog_procedure_manifest_resolution_request(&request)?;

        let selector = match request.selector {
            Some(GeneratedCatalogManifestSelector::ProcedureId(procedure_id)) => {
                CatalogManifestSelector::ProcedureId(ProcedureId::new(procedure_id))
            }
            Some(GeneratedCatalogManifestSelector::ProcedureName(procedure_name)) => {
                CatalogManifestSelector::ProcedureName(procedure_name)
            }
            None => {
                return Err(protocol_error(
                    "catalog manifest resolution request requires selector",
                ));
            }
        };
        let expected_contract_hash = request
            .expected_contract_hash
            .as_deref()
            .map(ContractHash::from_slice)
            .transpose()?;
        let expected_catalog_version = request.expected_catalog_version.map(CatalogVersion::new);

        Ok(Self {
            protocol_major: request.protocol_major,
            protocol_minor: request.protocol_minor,
            request_id: RequestId::new(request.request_id),
            trace_id: request.trace_id,
            selector,
            expected_contract_hash,
            expected_catalog_version,
            require_source_generator_ready: request.require_source_generator_ready,
        })
    }
}

/// Domain response returned by the catalog manifest runtime before protobuf encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionResponse {
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub request_id: RequestId,
    pub trace_id: Option<String>,
    pub status: CatalogManifestResolutionStatus,
    pub manifest: Option<CatalogProcedureManifest>,
    pub current_catalog_version: Option<CatalogVersion>,
    pub diagnostic_code: Option<String>,
}

impl CatalogManifestResolutionResponse {
    pub fn to_protobuf(&self) -> AndromedaResult<GeneratedCatalogManifestResolutionResponse> {
        if self.status == CatalogManifestResolutionStatus::Unspecified {
            return Err(protocol_error(
                "catalog manifest resolution status must be specified",
            ));
        }

        let manifest = self
            .manifest
            .as_ref()
            .map(CatalogProcedureManifest::to_protobuf)
            .transpose()?;
        let resolved = self.status == CatalogManifestResolutionStatus::Resolved;
        let resolved_contract_hash = if resolved {
            manifest
                .as_ref()
                .map(|manifest| manifest.contract_hash.clone())
        } else {
            None
        };
        let resolved_catalog_version = if resolved {
            manifest.as_ref().map(|manifest| manifest.catalog_version)
        } else {
            None
        };
        let response = GeneratedCatalogManifestResolutionResponse {
            protocol_major: self.protocol_major,
            protocol_minor: self.protocol_minor,
            request_id: self.request_id.get(),
            trace_id: self.trace_id.clone(),
            status: self.status as i32,
            manifest,
            resolved_contract_hash,
            resolved_catalog_version,
            current_catalog_version: self.current_catalog_version.map(CatalogVersion::get),
            diagnostic_code: self.diagnostic_code.clone(),
        };
        validate_catalog_procedure_manifest_resolution_response(&response)?;
        Ok(response)
    }

    pub fn validate_against_request(
        &self,
        request: &CatalogManifestResolutionRequest,
    ) -> AndromedaResult<()> {
        if self.status != CatalogManifestResolutionStatus::Resolved {
            return Ok(());
        }

        let Some(manifest) = self.manifest.as_ref() else {
            return Err(contract_error(
                "resolved catalog manifest response requires manifest evidence",
            ));
        };

        request.selector.validate_resolved_manifest(manifest)?;

        if let Some(expected_contract_hash) = request.expected_contract_hash
            && manifest.contract_hash != expected_contract_hash
        {
            return Err(contract_error(
                "resolved catalog manifest contract hash does not match request expectation",
            ));
        }

        if let Some(expected_catalog_version) = request.expected_catalog_version
            && manifest.catalog_version != expected_catalog_version
        {
            return Err(contract_error(
                "resolved catalog manifest catalog version does not match request expectation",
            ));
        }

        Ok(())
    }
}

/// Domain Procedure manifest used inside the QUIC catalog route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProcedureManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub protocol_layout: CatalogProcedureProtocolLayout,
    pub result_streams: Vec<CatalogResultStreamDescriptor>,
    pub stats_version: u64,
    pub policy_version: ContractHash,
    pub required_permissions: Vec<CatalogRequiredPermission>,
}

impl CatalogProcedureManifest {
    pub fn to_protobuf(&self) -> AndromedaResult<GeneratedProcedureManifest> {
        let manifest = GeneratedProcedureManifest {
            procedure_id: self.procedure_id.get(),
            procedure_name: self.procedure_name.clone(),
            contract_hash: self.contract_hash.as_bytes().to_vec(),
            catalog_version: self.catalog_version.get(),
            protocol_layout: Some(self.protocol_layout.to_protobuf()),
            result_streams: self
                .result_streams
                .iter()
                .map(CatalogResultStreamDescriptor::to_protobuf)
                .collect(),
            policy_version: self.policy_version.as_bytes().to_vec(),
            required_permissions: self
                .required_permissions
                .iter()
                .map(CatalogRequiredPermission::to_protobuf)
                .collect(),
            stats_version: Some(self.stats_version),
        };
        validate_catalog_procedure_manifest_resolution_response(
            &GeneratedCatalogManifestResolutionResponse {
                protocol_major: 1,
                protocol_minor: 0,
                request_id: 1,
                trace_id: None,
                status: CatalogManifestResolutionStatus::Resolved as i32,
                manifest: Some(manifest.clone()),
                resolved_contract_hash: Some(manifest.contract_hash.clone()),
                resolved_catalog_version: Some(manifest.catalog_version),
                current_catalog_version: Some(manifest.catalog_version),
                diagnostic_code: None,
            },
        )?;
        Ok(manifest)
    }
}

impl TryFrom<GeneratedProcedureManifest> for CatalogProcedureManifest {
    type Error = AndromedaError;

    fn try_from(manifest: GeneratedProcedureManifest) -> AndromedaResult<Self> {
        validate_catalog_procedure_manifest_resolution_response(
            &GeneratedCatalogManifestResolutionResponse {
                protocol_major: 1,
                protocol_minor: 0,
                request_id: 1,
                trace_id: None,
                status: CatalogManifestResolutionStatus::Resolved as i32,
                manifest: Some(manifest.clone()),
                resolved_contract_hash: Some(manifest.contract_hash.clone()),
                resolved_catalog_version: Some(manifest.catalog_version),
                current_catalog_version: Some(manifest.catalog_version),
                diagnostic_code: None,
            },
        )?;

        let Some(protocol_layout) = manifest.protocol_layout else {
            return Err(protocol_error(
                "resolved procedure manifest requires protocol layout",
            ));
        };
        let Some(stats_version) = manifest.stats_version else {
            return Err(protocol_error(
                "resolved procedure manifest requires stats version",
            ));
        };

        Ok(Self {
            procedure_id: ProcedureId::new(manifest.procedure_id),
            procedure_name: manifest.procedure_name,
            contract_hash: ContractHash::from_slice(&manifest.contract_hash)?,
            catalog_version: CatalogVersion::new(manifest.catalog_version),
            protocol_layout: CatalogProcedureProtocolLayout::try_from(protocol_layout)?,
            result_streams: manifest
                .result_streams
                .into_iter()
                .map(CatalogResultStreamDescriptor::from)
                .collect(),
            stats_version,
            policy_version: ContractHash::from_slice(&manifest.policy_version)?,
            required_permissions: manifest
                .required_permissions
                .into_iter()
                .map(CatalogRequiredPermission::from)
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProcedureProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
    pub protocol_package: String,
    pub contract_package: String,
}

impl CatalogProcedureProtocolLayout {
    fn to_protobuf(&self) -> GeneratedProtocolLayout {
        GeneratedProtocolLayout {
            descriptor_set_hash: self.descriptor_set_hash.as_bytes().to_vec(),
            frame_envelope_hash: self.frame_envelope_hash.as_bytes().to_vec(),
            protocol_package: self.protocol_package.clone(),
            contract_package: self.contract_package.clone(),
        }
    }
}

impl TryFrom<GeneratedProtocolLayout> for CatalogProcedureProtocolLayout {
    type Error = AndromedaError;

    fn try_from(layout: GeneratedProtocolLayout) -> AndromedaResult<Self> {
        Ok(Self {
            descriptor_set_hash: ContractHash::from_slice(&layout.descriptor_set_hash)?,
            frame_envelope_hash: ContractHash::from_slice(&layout.frame_envelope_hash)?,
            protocol_package: layout.protocol_package,
            contract_package: layout.contract_package,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRequiredPermission {
    pub id: String,
    pub family: String,
}

impl CatalogRequiredPermission {
    fn to_protobuf(&self) -> GeneratedRequiredPermission {
        GeneratedRequiredPermission {
            id: self.id.clone(),
            family: self.family.clone(),
        }
    }
}

impl From<GeneratedRequiredPermission> for CatalogRequiredPermission {
    fn from(permission: GeneratedRequiredPermission) -> Self {
        Self {
            id: permission.id,
            family: permission.family,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<CatalogColumnDescriptor>,
    pub cardinality: i32,
    pub row_count_requirement: i32,
    pub row_count_exact: Option<u64>,
    pub row_count_max: Option<u64>,
}

impl CatalogResultStreamDescriptor {
    fn to_protobuf(&self) -> GeneratedResultStreamDescriptor {
        GeneratedResultStreamDescriptor {
            stream_name: self.stream_name.clone(),
            columns: self
                .columns
                .iter()
                .map(CatalogColumnDescriptor::to_protobuf)
                .collect(),
            cardinality: self.cardinality,
            row_count_requirement: self.row_count_requirement,
            row_count_exact: self.row_count_exact,
            row_count_max: self.row_count_max,
        }
    }
}

impl From<GeneratedResultStreamDescriptor> for CatalogResultStreamDescriptor {
    fn from(stream: GeneratedResultStreamDescriptor) -> Self {
        Self {
            stream_name: stream.stream_name,
            columns: stream
                .columns
                .into_iter()
                .map(CatalogColumnDescriptor::from)
                .collect(),
            cardinality: stream.cardinality,
            row_count_requirement: stream.row_count_requirement,
            row_count_exact: stream.row_count_exact,
            row_count_max: stream.row_count_max,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogColumnDescriptor {
    pub name: String,
    pub ordinal: u32,
    pub type_name: String,
}

impl CatalogColumnDescriptor {
    fn to_protobuf(&self) -> GeneratedColumnDescriptor {
        GeneratedColumnDescriptor {
            name: self.name.clone(),
            ordinal: self.ordinal,
            type_name: self.type_name.clone(),
        }
    }
}

impl From<GeneratedColumnDescriptor> for CatalogColumnDescriptor {
    fn from(column: GeneratedColumnDescriptor) -> Self {
        Self {
            name: column.name,
            ordinal: column.ordinal,
            type_name: column.type_name,
        }
    }
}
