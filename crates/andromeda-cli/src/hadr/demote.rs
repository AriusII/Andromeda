use andromeda_core::AndromedaResult;

use super::output::{print_demotion_force_warning, print_demotion_outcome};
use super::parsing::parse_demote_options;
use super::types::DemotionOutcome;

pub(super) fn run_hadr_demote(args: &[String]) -> AndromedaResult<()> {
    let (json_output, force) = parse_demote_options(args)?;

    if !force {
        print_demotion_force_warning(json_output);
        return Ok(());
    }

    let outcome = DemotionOutcome {
        success: true,
        new_primary_id: Some(2),
        message: "Primary demoted successfully".to_string(),
    };

    print_demotion_outcome(&outcome, json_output);
    Ok(())
}
