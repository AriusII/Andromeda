use andromeda_error::AndromedaResult;

use super::output::{print_demotion_force_warning, print_demotion_outcome};
use super::parsing::parse_demote_options;
use super::types::DemotionOutcome;

pub(super) fn run_hadr_demote(args: &[String]) -> AndromedaResult<()> {
    let options = parse_demote_options(args)?;

    if !options.force {
        print_demotion_force_warning(options.json_output);
        return Ok(());
    }
    if !options.dry_run {
        return Err(crate::error::cli_error(
            "hadr demote is contract preview only until a durable HADR backend is wired; rerun with --dry-run",
        ));
    }

    let outcome = DemotionOutcome {
        success: true,
        dry_run: true,
        would_apply: false,
        contract_preview: true,
        new_primary_id: Some(2),
        message: "dry-run accepted: primary demotion would require durable epoch, quorum, fencing, and audit updates".to_string(),
    };

    print_demotion_outcome(&outcome, options.json_output);
    Ok(())
}
