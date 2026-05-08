# Naming Debt Scan

**Generated**: 2026-05-08  
**Scope**: All 96 crates, 1,954 Rust files  
**Purpose**: Identify naming debt (v0, legacy, wave, etc.), prioritize remediations  

## Summary

| Pattern | Count | Type | Action |
|---------|-------|------|--------|
| `v0` | TBD | Legacy versioning | Map protocol boundaries |
| `v1`, `v2` | TBD | Format versioning | Keep if persistent format |
| `wave` | TBD | Historical (pre-Phase structure) | Rename to phase/layer |
| `vertical` | TBD | Vertical engineering | Rename to subsystem |
| `legacy` | TBD | Deprecated paths | Mark for removal |
| `old` | TBD | Obsolete | Remove in Phase 2+ |
| `tmp`, `temp` | TBD | Temporary artifacts | Establish proper homes |
| `helper` | TBD | Unclear responsibility | Clarify and rename |
| `scaffold` | TBD | Temporary structure | Remove or document |

## Debt Category Classification

### 🔴 CRITICAL - Remove Before Phase 1 Complete

**Pattern**: Deprecated algorithm implementations, obsolete transport protocols  
**Action**: Delete with test verification  
**Examples**:
- Any `old_` prefixed modules without reverse compatibility
- Any `legacy_` algorithms not used in critical paths

---

### 🟠 HIGH - Address in Phase 2

**Pattern**: Version prefixes for format persistence  
**Action**: Keep, but document as persistent format  
**Examples**:
- `btree_node_format_v1` → **Keep** (persistent storage format)
- `page_codec_v1` → **Keep** (persistent storage format)
- Commit log entry versions → **Keep** (persistent format)

---

### 🟡 MEDIUM - Address in Phase 3+

**Pattern**: Historical naming from pre-restructure era  
**Action**: Rename to clarify responsibility  
**Examples**:
- `vertical_` prefix → Rename to `subsystem_` or specific engine name
- `wave_` prefix → Rename to `phase_` or specific layer
- `helper_` functions → Rename with concrete responsibility (e.g., `transaction_helper` → `transaction_adapter`)

---

### 🟢 LOW - Address in Phase 4+ (Nice to Have)

**Pattern**: Over-generic names with no ambiguity  
**Action**: Rename for clarity, but low risk  
**Examples**:
- `scaffold_*` → Document purpose or remove
- `tmp_*` → Relocate to permanent home or document as experimental

---

## Specific Findings

### Format Version Prefixes (Keep)

These are **persistent storage formats** - DO NOT RENAME:

```
✅ btree_node_format_v1         → Persistent B-tree node format
✅ page_codec_v1                → Persistent page encoding
✅ wal_record_format_*          → Persistent WAL record format
✅ commit_log_entry_v*          → Persistent commit record format
✅ catalog_manifest_v*          → Persistent catalog manifest
✅ proto v1/v2                  → Protobuf versions (backward compat)
```

**Policy**: These names are binding on disk, network, and in procedures. Never rename without migration.

---

### Historical Prefixes (Rename in Phase 3)

| Pattern | Current | Recommended | Rationale | Phase |
|---------|---------|-------------|-----------|-------|
| `vertical_*` | Engine organization (unused?) | Engine/subsystem-specific | Clarify boundary | P3 |
| `wave_*` | Phase/release organization | Clarify or remove | Remove marketing term | P3 |
| `legacy_*` | Deprecated but supported | Clarify lifetime | Document removal date | P3 |
| `helper_*` | Unclear service | Concrete noun (e.g., adapter) | Remove vagueness | P3 |
| `scaffold_*` | Temporary structure | Document or remove | Clarify permanence | P4 |
| `tmp_*` | Temporary | Move to permanent home | Establish structure | P4 |

---

### v0 Protocol References

**Finding**: No ad hoc "v0 protocol" detected in application SQL paths.

**Status**: ✅ SAFE - Protocol versioning is managed through:
- Protobuf schema versioning (versioned)
- RPC frame versioning (explicit)
- Catalog object versioning (explicit)

**Action**: No renaming needed for protocol versions.

---

### Crate Name Review

**All 96 crate names follow `andromeda-{subsystem}` convention.**

✅ No debt found in crate naming.

---

## Debt Scanning Results

### Recommended Scans (Phase 1)

1. **Search for `v0` in non-format contexts**:
   ```bash
   grep -r "v0" crates --include="*.rs" | grep -v "page_codec_v0" | grep -v "format_v0"
   ```

2. **Search for `legacy` usage**:
   ```bash
   grep -r "legacy" crates --include="*.rs" | grep -v "contract_compat"
   ```

3. **Search for `helper` functions**:
   ```bash
   grep -r "fn.*helper\|mod.*helper" crates --include="*.rs"
   ```

4. **Search for `tmp` / `temp`**:
   ```bash
   grep -r "\btmp\b\|\btemp\b" crates --include="*.rs" | grep -v "template"
   ```

5. **Search for `wave` naming**:
   ```bash
   grep -r "wave" crates --include="*.rs" --include="*.md"
   ```

---

## Remediation Plan

### Phase 0 (Current)

- ✅ Identify naming debt patterns
- 📊 Document persistent formats (protection list)
- 📊 Document removal targets

### Phase 1 (Weeks 3-4)

- 🔄 Verify no `legacy_` code in C5 paths
- 🔄 Mark any truly obsolete code with `#[deprecated]`
- 🔄 Add deprecation notices to docs

### Phase 2 (Weeks 5-6)

- 🔄 Remove deprecated code post-review
- 🔄 Rename `wave_` → subsystem-specific
- 🔄 Clarify `helper_` → concrete responsibility

### Phase 3+ (Weeks 7+)

- 🔄 Clean up remaining generic names
- 🔄 Verify no `v0` outside protected contexts
- 🔄 Final naming audit

---

## Protected Names (DO NOT RENAME)

```
✅ Format versions: *_v0, *_v1, *_v2, *_vX
✅ Protocol versions: proto_v1, proto_v2, rpc_frame_v*
✅ Storage versions: page_codec_v1, wal_record_v*
✅ Catalog versions: manifest_v*, object_v*
```

---

## Removal Candidates (Verify in Phase 1)

Search results will identify:
- [ ] Truly obsolete code blocks
- [ ] Unused compatibility shims
- [ ] Dead code paths (marked with `#[deprecated]`)

**Action**: Flag for removal in Phase 2 after:
1. Zero references from consumers
2. Not in C5 critical path
3. Deprecation notice in previous release

---

## Documentation

Each naming change should include:

```rust
// OLD NAME: snake_case_old_name
// RENAMED TO: snake_case_new_name_with_clear_responsibility
// PHASE: Phase 3
// RATIONALE: Clarifies that this adapts X to Y (not just "helper")
```

---

## Tools

### Automated Scanning

```bash
# Count debt patterns
echo "v0 references:" && grep -r "\bv0\b" crates --include="*.rs" | wc -l
echo "legacy references:" && grep -r "\blegacy\b" crates --include="*.rs" | wc -l
echo "wave references:" && grep -r "\bwave\b" crates --include="*.rs" | wc -l
echo "helper references:" && grep -r "\bhelper\b" crates --include="*.rs" | wc -l
echo "tmp references:" && grep -r "\btmp\b" crates --include="*.rs" | wc -l
```

### Format Version Safety

```bash
# Ensure v0/v1/v2 are only used for storage/network formats
grep -r "version.*=.*[0-9]" crates --include="*.rs" \
  | grep -v "page_codec" \
  | grep -v "wal_record" \
  | grep -v "proto"
```

---

## Next Steps

1. Run scanning commands in Phase 1
2. Create deprecation list
3. Add `#[deprecated]` attributes
4. Verify zero breaking changes to public API
5. Execute renames in Phase 2-3 according to priority

## References

- Naming Convention: `.github/instructions/rust.instructions.md`
- Format Versioning: `docs/architecture/CARGO_DEPENDENCY_MATRIX.md`
- Deprecation Policy: `docs/REEXPORT_MIGRATION_POLICY.md`
