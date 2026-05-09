//! Facade adapters for executable SRPL catalog binding.

use andromeda_catalog::{CatalogDefinition, CatalogSnapshot, ProcedureContract, QualifiedName};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl_binder::{
    SrplCatalogStructuredObjectBinding, SrplCatalogTableBinding, SrplExecutableCatalogView,
};
use andromeda_srpl_diagnostics::{DiagnosticPhase, SrplDiagnostic};
use andromeda_srpl_ir::{ExecutableProcedurePlan, SrplProcedureBodyIr, SrplProcedureIr};
use andromeda_types::CatalogVersion;

pub fn bind_executable_procedure_plan(
    ir: &SrplProcedureIr,
    catalog: &CatalogSnapshot,
) -> AndromedaResult<ExecutableProcedurePlan> {
    andromeda_srpl_binder::bind_executable_procedure_plan(
        ir,
        &CatalogSnapshotBindingView { catalog },
    )
}

pub fn inventory_reserve_stock_body_ir() -> Result<SrplProcedureBodyIr, SrplDiagnostic> {
    andromeda_srpl_binder::inventory_reserve_stock_body_ir()
        .map_err(|error| SrplDiagnostic::new(DiagnosticPhase::IrLowering, None, error.to_string()))
}

struct CatalogSnapshotBindingView<'a> {
    catalog: &'a CatalogSnapshot,
}

impl SrplExecutableCatalogView for CatalogSnapshotBindingView<'_> {
    fn catalog_version(&self) -> CatalogVersion {
        self.catalog.version
    }

    fn lookup_procedure_contract(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<ProcedureContract> {
        match self.catalog.get_by_name(name) {
            Some(CatalogDefinition::Procedure(contract))
                if self.catalog.is_active_object(contract.object.object_id) =>
            {
                Ok(contract.clone())
            },
            Some(CatalogDefinition::Procedure(_)) => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan cannot bind a deprecated procedure contract",
            )),
            Some(_) => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan procedure name resolved to a non-procedure object",
            )),
            None => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan cannot bind missing procedure contract",
            )),
        }
    }

    fn lookup_table(&self, name: &QualifiedName) -> AndromedaResult<SrplCatalogTableBinding> {
        match self.catalog.get_by_name(name) {
            Some(CatalogDefinition::Table(table))
                if self.catalog.is_active_object(table.object.object_id) =>
            {
                Ok(SrplCatalogTableBinding {
                    object: table.object.clone(),
                    columns: table.columns.clone(),
                    shape_hash: table.shape_hash(),
                })
            },
            Some(CatalogDefinition::Table(_)) => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan cannot bind a deprecated table",
            )),
            Some(_) => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL body table name resolved to a non-table object",
            )),
            None => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL body references an unbound table/object name",
            )),
        }
    }

    fn try_lookup_structured_object(
        &self,
        name: &QualifiedName,
    ) -> AndromedaResult<Option<SrplCatalogStructuredObjectBinding>> {
        match self.catalog.get_by_name(name) {
            Some(CatalogDefinition::StructuredObject(object))
                if self.catalog.is_active_object(object.object.object_id) =>
            {
                Ok(Some(SrplCatalogStructuredObjectBinding {
                    object: object.object.clone(),
                    fields: object.fields.clone(),
                    shape_hash: object.shape_hash(),
                }))
            },
            Some(CatalogDefinition::StructuredObject(_)) => Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan cannot bind a deprecated structured object",
            )),
            Some(_) | None => Ok(None),
        }
    }
}
