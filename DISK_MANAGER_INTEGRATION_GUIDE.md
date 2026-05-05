# Integration Guide: DiskManager with Buffer Pool

## Overview

The DiskManager implementation provides real disk I/O backing for the buffer pool. This guide explains how to integrate DiskManager with the existing buffer pool architecture.

## Current State

### PageStore Trait (page.rs)
```rust
pub trait PageStore {
    fn page_size(&self) -> PageSize;
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>>;
    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()>;
    fn allocate_page(...) -> AndromedaResult<PageImage>;
}
```

### Current Implementation
- `InMemoryPageStore` - BTreeMap-based (test-only, mocks I/O)
- Used by `BufferPool<S: PageStore>` for all page operations

### Buffer Pool Flush Flow
```
buffer_pool.flush_dirty_frames(observer)
  ├─ iterate dirty_tracker.flush_candidates()
  ├─ check observer.is_durable(first_dirty_lsn)
  └─ call store.write_page(image, first_dirty_lsn)
      └─ InMemoryPageStore::write_page (currently)
          └─ Updates BTreeMap in RAM (NO DISK I/O)
```

## Integration Steps

### Step 1: Create DiskPageStore Adapter

The `DiskPageStore` wrapper (already stubbed in disk_manager.rs) implements the PageStore trait:

```rust
pub struct DiskPageStore {
    manager: FileDiskManager,
}

impl PageStore for DiskPageStore {
    fn page_size(&self) -> PageSize {
        // Track in FileDiskManager or derive from config
    }
    
    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        self.manager.read_page(page_id)
    }
    
    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        // Validate WAL-before-page-flush precondition
        let page_lsn = image.page_lsn().ok_or(...)?;
        if durable_lsn < page_lsn {
            return Err(storage_error("WAL LSN behind page LSN"));
        }
        self.manager.write_page(image, durable_lsn)
    }
    
    fn allocate_page(...) -> AndromedaResult<PageImage> {
        // Allocate through extent manager
        // Then create and return PageImage
    }
}
```

### Step 2: Extent Registration

During startup, load extent metadata from manifest and register with DiskManager:

```rust
// In recovery/startup sequence
let mut disk_page_store = DiskPageStore::new(data_file_path, temp_dir)?;

// Load extents from manifest
for extent in manifest.all_extents() {
    disk_page_store.disk_manager_mut().register_extent(extent)?;
}

// Create buffer pool with disk-backed store
let buffer_pool = BufferPool::new(config, disk_page_store)?;
```

### Step 3: Update Page Allocation

When allocating new pages, extents must be created first:

```rust
// In execution engine or catalog manager
let extent = ExtentDescriptor {
    extent_id: new_extent_id(),
    object_id,
    allocation_id,
    first_page_id: next_page_id(),
    page_count: 100, // e.g., 100 pages per extent
    page_size: PageSize::KiB16,
    state: ExtentState::AllocatingHot,
    segment_id: None,
    file_offset: 0,         // Set by DiskManager.allocate_extent()
    allocated_on_disk: false,
};

disk_page_store.disk_manager_mut().allocate_extent(extent)?;

// Now pages within [first_page_id, first_page_id + page_count - 1] can be allocated
// via buffer_pool.new_page()
```

## Data Flow After Integration

### Page Flush (Write Path)
```
buffer_pool.flush_dirty_frames(observer)
  ├─ observer.is_durable(first_dirty_lsn) ✓
  ├─ store.write_page(image, first_dirty_lsn)
  │   └─ DiskPageStore::write_page()
  │       ├─ Validate page_lsn ≤ durable_lsn
  │       └─ manager.write_page(image, durable_lsn)
  │           ├─ Look up extent for page_id
  │           ├─ Compute file offset: extent.file_offset + offset_within_extent
  │           ├─ Write to .tmp file
  │           ├─ Compute CRC32
  │           ├─ fsync(.tmp)
  │           ├─ Write to main datastore.bin at computed offset
  │           ├─ fsync(datastore.bin)
  │           └─ Delete .tmp file
  └─ dirty_tracker.mark_clean(page_id)
```

### Page Load (Read Path)
```
buffer_pool.fetch_page(page_id)
  └─ ensure_resident(page_id)
      ├─ Check page_table cache
      └─ On miss: store.read_page(page_id)
          └─ DiskPageStore::read_page()
              └─ manager.read_page(page_id)
                  ├─ Look up extent for page_id
                  ├─ Compute file offset
                  ├─ Seek and read from datastore.bin
                  ├─ Validate CRC32
                  └─ Return PageImage
```

## File Layout Example

After allocating three extents:

```
datastore.bin (append-only)
├─ Offset 0:        Extent 1 (pages 1-100, 16 KiB each)
│                   [1.6 MiB total]
├─ Offset 1.6 MiB:  Extent 2 (pages 101-200, 16 KiB each)
│                   [1.6 MiB total]
└─ Offset 3.2 MiB:  Extent 3 (pages 201-250, 32 KiB each)
                    [1.6 MiB total]
```

Manifest tracks:
```json
{
  "extents": [
    { "extent_id": 1, "first_page_id": 1, "page_count": 100, "file_offset": 0, ... },
    { "extent_id": 2, "first_page_id": 101, "page_count": 100, "file_offset": 1671168, ... },
    { "extent_id": 3, "first_page_id": 201, "page_count": 50, "file_offset": 3342336, ... }
  ]
}
```

## Crash Safety Guarantees

### During Write

If process crashes **during `write_page()`**:

1. **Before fsync(.tmp):** .tmp file may contain partial write
   - Safe: not yet visible; orphaned temp file cleaned on restart

2. **After fsync(.tmp), before seek(main):** Crash safe
   - .tmp has full page, fsync'd
   - Main file unchanged
   - Next read still gets old page

3. **After write(main), before fsync(main):** Crash safe
   - OS buffer might be lost but we wrote to OS buffer cache
   - fsync() ensures durability

4. **After fsync(main), before delete(.tmp):** Safe
   - Main file has new page (fsync'd to disk)
   - .tmp file orphaned (cleaned on restart)

**Result:** Main file always has complete page; never partial write.

### On Restart

1. Scan temp_dir for orphaned .tmp files
2. Delete them (they represent failed or completed writes)
3. Load extent metadata from manifest
4. Register extents with DiskManager
5. Resume normal operation

## Performance Considerations

### Current (MVP)
- **Synchronous I/O:** `write_all()` and `sync_all()` block
- **One .tmp per write:** Creates/deletes temp file per flush
- **No buffering:** Each page write is a separate system call

### Optimizations (Future)
- Use `tokio::fs` for async I/O
- Batch multiple pages into single write
- Use buffered writer with flush batching
- Memory-map hot extents for random access

## Error Scenarios

| Scenario | Handling |
|----------|----------|
| Page ID not allocated | Returns `Ok(None)` on read; error on write |
| File I/O fails (permission, disk full) | Propagates `AndromedaError::Storage` |
| CRC mismatch on read | Returns `PageCorrupted` error |
| Overlapping extents | Rejected at allocation time |
| .tmp file remains on crash | Cleaned during recovery |

## Testing Integration

### Unit Test Example
```rust
#[test]
fn test_buffer_pool_with_disk_backing() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create disk-backed page store
    let mut disk_store = DiskPageStore::new(
        temp_dir.path().join("test.bin"),
        temp_dir.path(),
    ).unwrap();
    
    // Allocate extent
    let extent = create_test_extent();
    disk_store.disk_manager_mut().allocate_extent(extent).unwrap();
    
    // Create buffer pool
    let mut pool = BufferPool::new(config, disk_store).unwrap();
    
    // Allocate and modify page
    let (page_id, mut guard) = pool.new_page(layout_contract).unwrap();
    // ... modify page ...
    drop(guard);
    
    // Flush
    pool.flush_dirty_frames(&observer).unwrap();
    
    // Verify page on disk
    assert_page_on_disk(page_id);
}
```

## Manifest Integration

The manifest must track extent metadata. Schema should include:

```rust
pub struct ExtentMetadata {
    pub extent_id: u64,
    pub object_id: u64,
    pub allocation_id: u64,
    pub first_page_id: u64,
    pub page_count: u32,
    pub page_size: PageSize,
    pub state: ExtentState,
    pub segment_id: Option<u64>,
    pub file_offset: u64,          // CRITICAL: persistence point
    pub allocated_on_disk: bool,
    pub created_lsn: Lsn,
}
```

**Key point:** `file_offset` is computed once at allocation and must be persistent. Recovery replays it exactly.

## Handoff Checklist

- [ ] Complete DiskPageStore impl (PageStore adapter)
- [ ] Test with actual BufferPool in integration suite
- [ ] Verify manifest serialization of extent metadata
- [ ] Implement startup recovery (extent reloading)
- [ ] Add .tmp file cleanup on recovery
- [ ] Performance baseline (I/O latency, throughput)
- [ ] Crash simulation tests
- [ ] Replace InMemoryPageStore in production builds

---

**Next: Phase 2 - DiskPageStore Implementation & Integration**
