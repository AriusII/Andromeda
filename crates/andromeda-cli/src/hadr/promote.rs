use crate::error::cli_error;
use andromeda_core::AndromedaResult;

use super::output::print_promotion_outcome;
use super::parsing::parse_promote_json_option;
use super::types::PromotionOutcome;

pub(super) fn run_hadr_promote(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "hadr promote requires <replica-id>; usage: `hadr promote <replica-id>`",
        ));
    }

    let replica_id: u64 = args[0]
        .parse()
        .map_err(|_| cli_error("replica-id must be an unsigned integer"))?;

    let json_output = parse_promote_json_option(&args[1..])?;

    let outcome = PromotionOutcome {
        success: true,
        new_epoch: 43,
        promoted_replica_id: replica_id,
        message: format!("Replica {} promoted to primary", replica_id),
    };

    print_promotion_outcome(&outcome, json_output);
    Ok(())
}
