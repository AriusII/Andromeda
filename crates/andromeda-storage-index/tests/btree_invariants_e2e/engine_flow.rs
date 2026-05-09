use andromeda_storage_index::{BTreeConfig, InMemoryBTreeIndexEngine, IndexId, PageId, RowId};

#[test]
fn engine_insert_lookup_delete_and_range_are_ordered_e2e() {
    let mut engine =
        InMemoryBTreeIndexEngine::new(IndexId::new(7), PageId::new(700), BTreeConfig::default());

    for key in [40u8, 10, 30, 20, 50] {
        engine
            .insert(&[key], RowId::new(key as u64 * 10))
            .expect("insert should accept unique in-range keys");
    }

    assert_eq!(engine.row_count(), 5);
    assert_eq!(engine.search(&[10]).unwrap(), Some(RowId::new(100)));
    assert_eq!(engine.search(&[25]).unwrap(), None);
    assert!(
        engine.insert(&[20], RowId::new(999)).is_err(),
        "duplicate keys must not overwrite the existing row locator"
    );

    let scanned: Vec<u64> = engine
        .range_scan(&[15], &[45])
        .unwrap()
        .into_iter()
        .map(RowId::get)
        .collect();
    assert_eq!(scanned, vec![200, 300, 400]);

    engine.delete(&[30]).unwrap();
    assert_eq!(engine.search(&[30]).unwrap(), None);
    assert_eq!(engine.row_count(), 4);
    assert_eq!(engine.statistics().total_key_count, 4);
}
