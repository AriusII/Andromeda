//! Regression guard for the remaining `andromeda_storage::layout::page`
//! compatibility surface.
//!
//! Non-page layout facades were pruned. This test now pins only the active
//! compatibility path still consumed by page ownership checks.

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
                "layout re-export type `",
                stringify!($facade_path),
                "` diverged from root canonical `",
                stringify!($root_path),
                "`; the layout module must remain a pure `pub use` surface.",
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

/// Sanity check: a value built through the root path is interchangeable with
/// the re-export path. This is a compile-time enforcement that the two paths name
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
