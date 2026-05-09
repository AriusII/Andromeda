use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_error::AndromedaResult;

pub(super) struct ListProceduresOptions {
    pub(super) namespace: Option<String>,
    pub(super) json_output: bool,
}

pub(super) struct InvalidateCacheOptions {
    pub(super) procedure_id: Option<u64>,
    pub(super) json_output: bool,
}

pub(super) struct ShowContractOptions {
    pub(super) procedure_id: u64,
    pub(super) json_output: bool,
}

pub(super) struct ResolveManifestOptions {
    pub(super) selector: ManifestSelector,
    pub(super) json_output: bool,
}

pub(super) enum ManifestSelector {
    ProcedureId(u64),
    QualifiedName(String),
}

pub(super) fn parse_list_procedures_options(
    args: &[String],
) -> AndromedaResult<ListProceduresOptions> {
    let mut namespace: Option<String> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--namespace" => {
                namespace = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--namespace requires a namespace argument",
                    )?
                    .to_string(),
                );
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown list-procedures option; supported options are --namespace and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected list-procedures argument; supported options are --namespace and --json",
                ));
            },
        }
        i += 1;
    }

    Ok(ListProceduresOptions {
        namespace,
        json_output,
    })
}

pub(super) fn parse_invalidate_cache_options(
    args: &[String],
) -> AndromedaResult<InvalidateCacheOptions> {
    let mut procedure_id: Option<u64> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--procedure-id" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--procedure-id requires a procedure ID argument",
                )?;
                procedure_id = Some(parse_u64(
                    value,
                    "procedure-id must be an unsigned integer",
                )?);
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown invalidate-cache option; supported options are --procedure-id and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected invalidate-cache argument; supported options are --procedure-id and --json",
                ));
            },
        }
        i += 1;
    }

    Ok(InvalidateCacheOptions {
        procedure_id,
        json_output,
    })
}

pub(super) fn parse_show_contract_options(args: &[String]) -> AndromedaResult<ShowContractOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "show-contract requires <procedure-id>; usage: `catalog show-contract <procedure-id>`",
        ));
    }

    let procedure_id = parse_u64(&args[0], "procedure-id must be an unsigned integer")?;

    let mut json_output = false;
    for arg in &args[1..] {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown catalog show-contract option; supported option is --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected catalog show-contract argument; supported option is --json",
                ));
            },
        }
    }

    Ok(ShowContractOptions {
        procedure_id,
        json_output,
    })
}

pub(super) fn parse_resolve_manifest_options(
    args: &[String],
) -> AndromedaResult<ResolveManifestOptions> {
    let mut procedure_id: Option<u64> = None;
    let mut qualified_name: Option<String> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--procedure-id" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--procedure-id requires a procedure ID argument",
                )?;
                procedure_id = Some(parse_u64(
                    value,
                    "procedure-id must be an unsigned integer",
                )?);
            },
            "--qualified-name" => {
                qualified_name = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--qualified-name requires a Procedure qualified name argument",
                    )?
                    .to_string(),
                );
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown resolve-manifest option; supported options are --procedure-id, --qualified-name, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected resolve-manifest argument; supported options are --procedure-id, --qualified-name, and --json",
                ));
            },
        }
        i += 1;
    }

    let selector = match (procedure_id, qualified_name) {
        (Some(procedure_id), None) => ManifestSelector::ProcedureId(procedure_id),
        (None, Some(qualified_name)) => ManifestSelector::QualifiedName(qualified_name),
        _ => {
            return Err(cli_error(
                "resolve-manifest requires exactly one selector: --procedure-id <id> or --qualified-name <name>",
            ));
        },
    };

    Ok(ResolveManifestOptions {
        selector,
        json_output,
    })
}
