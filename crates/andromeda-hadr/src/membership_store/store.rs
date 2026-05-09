use andromeda_error::AndromedaResult;

use crate::{
    Lsn,
    membership_store::HadrMembershipSnapshot,
    types::{HadrEpoch, HadrNodeId, HadrNodeRole},
};

/// Durable membership operations used by runtime storage code.
pub trait HadrMembershipStore {
    fn load(&self) -> AndromedaResult<Option<HadrMembershipSnapshot>>;

    fn register_node(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot>;

    fn update_node_role(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot>;

    fn deregister_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot>;

    fn fence_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot>;

    fn advance_epoch(&self, target_epoch: HadrEpoch) -> AndromedaResult<HadrMembershipSnapshot>;

    fn promote_primary(
        &self,
        id: HadrNodeId,
        target_epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    ) -> AndromedaResult<HadrMembershipSnapshot>;
}
