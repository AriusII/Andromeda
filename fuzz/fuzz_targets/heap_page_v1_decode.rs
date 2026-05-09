#![no_main]

use andromeda_storage::HeapPage;
use andromeda_storage_page::PageSize;
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_HEAP_PAGE_FUZZ_BYTES: usize = 32 * 1024;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_HEAP_PAGE_FUZZ_BYTES);
    let page_size = selected_page_size(data);
    let image = page_image(page_size, data);
    exercise_heap_page_decode(page_size, &image);

    if data.len() == PageSize::KiB16.bytes_usize() {
        exercise_heap_page_decode(PageSize::KiB16, data);
    }
    if data.len() == PageSize::KiB32.bytes_usize() {
        exercise_heap_page_decode(PageSize::KiB32, data);
    }
});

fn selected_page_size(data: &[u8]) -> PageSize {
    if data.first().is_some_and(|byte| byte & 1 == 1) {
        PageSize::KiB32
    } else {
        PageSize::KiB16
    }
}

fn page_image(page_size: PageSize, data: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; page_size.bytes_usize()];
    let prefix = common::bounded_input(data, image.len());
    image[..prefix.len()].copy_from_slice(prefix);
    image
}

fn exercise_heap_page_decode(page_size: PageSize, image: &[u8]) {
    let Ok(page) = HeapPage::from_image(page_size, image) else {
        return;
    };

    assert_eq!(page.page_size(), page_size);
    assert!(page.slot_count() <= max_slots(page_size));
    assert!(page.live_row_count() <= page.slot_count() as u32);

    for slot_id in 0..page.slot_count().min(32) {
        let _ = page.read_tuple(slot_id as u16);
    }
}

const fn max_slots(page_size: PageSize) -> usize {
    match page_size {
        PageSize::KiB16 => 256,
        PageSize::KiB32 => 512,
    }
}
