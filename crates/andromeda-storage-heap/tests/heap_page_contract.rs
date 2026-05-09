#![forbid(unsafe_code)]

//! Heap page and slot directory contract tests split by durable format area.

#[path = "heap_page_contract/durable_layout.rs"]
mod durable_layout;
#[path = "heap_page_contract/image_validation.rs"]
mod image_validation;
#[path = "heap_page_contract/slot_directory.rs"]
mod slot_directory;
#[path = "heap_page_contract/support.rs"]
mod support;
