//! Regression guard: `andromeda_storage::layout::*` must remain a pure
//! facade over the root storage modules.
//!
//! The plan/Wave 1 doctrine pins the root modules (`page`, `extent`,
//! `segment`, `cold_store`, `io_budget`, `placement`) as the single source of
//! truth for storage layout types. Any future attempt to re-define a layout
//! type inside `src/layout/*` (instead of `pub use`-ing it from the root)
//! would diverge the root and facade `TypeId`s and fail this test, and would
//! also typically fail to compile the `assert_same_type` calls because
//! function-argument types would no longer unify.

use std::any::TypeId;

use andromeda_storage as root;
use andromeda_storage::layout;

/// Compile-time + run-time check: both paths resolve to the same nominal type.
fn assert_same_type<T: 'static>() -> TypeId {
    TypeId::of::<T>()
}

macro_rules! assert_facade_alias {
    ($root_path:path, $facade_path:path) => {{
        // The two `assert_same_type::<_>` calls are independent monomorphizations,
        // so this also asserts at compile-time that both paths name the same
        // concrete type (otherwise no single `T` could satisfy the call sites
        // that mix them, see the swap below).
        let root_id = assert_same_type::<$root_path>();
        let facade_id = assert_same_type::<$facade_path>();
        assert_eq!(
            root_id, facade_id,
            concat!(
                "layout facade type `",
                stringify!($facade_path),
                "` diverged from root canonical `",
                stringify!($root_path),
                "`; the layout module must remain a pure `pub use` facade.",
            )
        );
    }};
}

#[test]
fn layout_page_module_is_pure_facade_over_root() {
    assert_facade_alias!(root::PageId, layout::page::PageId);
    assert_facade_alias!(root::ObjectId, layout::page::ObjectId);
    assert_facade_alias!(root::AllocationId, layout::page::AllocationId);
    assert_facade_alias!(root::PageSize, layout::page::PageSize);
    assert_facade_alias!(root::PageType, layout::page::PageType);
    assert_facade_alias!(root::PageFlags, layout::page::PageFlags);
    assert_facade_alias!(root::PageHeader, layout::page::PageHeader);
    assert_facade_alias!(root::PageTrailer, layout::page::PageTrailer);
    assert_facade_alias!(root::PageLayoutContract, layout::page::PageLayoutContract);
}

#[test]
fn layout_extent_module_is_pure_facade_over_root() {
    assert_facade_alias!(root::ExtentId, layout::extent::ExtentId);
    assert_facade_alias!(root::ExtentState, layout::extent::ExtentState);
    assert_facade_alias!(root::ExtentDescriptor, layout::extent::ExtentDescriptor);
}

#[test]
fn layout_segment_module_is_pure_facade_over_root() {
    assert_facade_alias!(root::SegmentId, layout::segment::SegmentId);
    assert_facade_alias!(root::SegmentState, layout::segment::SegmentState);
    assert_facade_alias!(root::SegmentHeader, layout::segment::SegmentHeader);
    assert_facade_alias!(root::SegmentTrailer, layout::segment::SegmentTrailer);
    assert_facade_alias!(root::SegmentDescriptor, layout::segment::SegmentDescriptor);
    assert_facade_alias!(root::SegmentMutation, layout::segment::SegmentMutation);
}

#[test]
fn layout_cold_module_is_pure_facade_over_root() {
    assert_facade_alias!(
        root::PublishedColdSegment,
        layout::cold::PublishedColdSegment
    );
}

#[test]
fn layout_io_budget_module_is_pure_facade_over_root() {
    assert_facade_alias!(root::IoPathClass, layout::io_budget::IoPathClass);
    assert_facade_alias!(root::IoUseClass, layout::io_budget::IoUseClass);
    assert_facade_alias!(root::IoLatencyBudget, layout::io_budget::IoLatencyBudget);
    assert_facade_alias!(
        root::IoThroughputBudget,
        layout::io_budget::IoThroughputBudget
    );
    assert_facade_alias!(root::IoPathBudget, layout::io_budget::IoPathBudget);
    assert_facade_alias!(
        root::HotColdIoThresholds,
        layout::io_budget::HotColdIoThresholds
    );
    assert_facade_alias!(root::PageIoBudget, layout::io_budget::PageIoBudget);
    assert_facade_alias!(root::SegmentIoBudget, layout::io_budget::SegmentIoBudget);
}

#[test]
fn layout_placement_module_is_pure_facade_over_root() {
    assert_facade_alias!(
        root::CoreIoPlacementDecision,
        layout::placement::CoreIoPlacementDecision
    );
    assert_facade_alias!(
        root::CoreIoPlacementPolicy,
        layout::placement::CoreIoPlacementPolicy
    );
    assert_facade_alias!(
        root::CoreIoPlacementRequest,
        layout::placement::CoreIoPlacementRequest
    );
    assert_facade_alias!(root::DataTemperature, layout::placement::DataTemperature);
    assert_facade_alias!(root::PipelineStage, layout::placement::PipelineStage);
    assert_facade_alias!(
        root::PlacementDecision,
        layout::placement::PlacementDecision
    );
    assert_facade_alias!(
        root::ReadFallbackPolicy,
        layout::placement::ReadFallbackPolicy
    );
    assert_facade_alias!(
        root::StorageIoBudgetScope,
        layout::placement::StorageIoBudgetScope
    );
    assert_facade_alias!(
        root::StoragePlacementPolicy,
        layout::placement::StoragePlacementPolicy
    );
    assert_facade_alias!(root::StorageTier, layout::placement::StorageTier);
    assert_facade_alias!(
        root::StorageWorkloadClass,
        layout::placement::StorageWorkloadClass
    );
}

/// Sanity check: a value built through the root path is interchangeable with
/// the facade path. This is a compile-time enforcement that the two paths name
/// the same type; if anyone re-defines `PageId` inside `layout/page.rs`, this
/// function will fail to type-check.
#[test]
fn layout_facade_values_unify_with_root_values() {
    fn take_root(_id: root::PageId) {}
    fn take_facade(_id: layout::page::PageId) {}

    let from_root = root::PageId::new(7);
    let from_facade = layout::page::PageId::new(7);

    take_root(from_facade);
    take_facade(from_root);
}
