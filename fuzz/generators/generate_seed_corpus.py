#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import pathlib
import re
from typing import Dict, List


@dataclass(frozen=True)
class TargetSpec:
    name: str
    path: str
    corpus_dir: str
    generator: str


@dataclass(frozen=True)
class CargoBinSpec:
    name: str
    path: str


@dataclass(frozen=True)
class SupportFileSpec:
    path: str
    used_by: List[str]


@dataclass(frozen=True)
class ManifestEntry:
    target: str
    corpus_dir: str
    seed_files: List[str]
    generator: str


TARGETS_SCHEMA_VERSION = "andromeda-fuzz-targets-v1"
CORPUS_SCHEMA_VERSION = "andromeda-fuzz-corpus-v1"
GENERATOR_PATH = "fuzz/generators/generate_seed_corpus.py"

LEGACY_SEED1_PAYLOADS: Dict[str, bytes] = {
    "frame_codec_no_panic": b"frame\r\n",
    "result_sequence_state_machine": b"result-sequence\r\n",
    "proto_frame_envelope_decode": b"proto-envelope\r\n",
    "proto_rpc_completion_decode": b"completion\r\n",
    "srpl_parser_signature_decode": b"PROC demo(a:int)->rows\r\n",
    "storage_wal_record_roundtrip": b"wal\r\n",
}


def load_targets(path: pathlib.Path) -> List[str]:
    return [target.name for target in load_target_specs(path)]


def load_top_level_strings(path: pathlib.Path) -> Dict[str, str]:
    text = path.read_text(encoding="utf-8")
    fields: Dict[str, str] = {}
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            break
        match = re.match(r'^\s*([A-Za-z0-9_-]+)\s*=\s*"([^"]*)"\s*$', line)
        if match:
            fields[match.group(1)] = match.group(2)
    return fields


def load_target_specs(path: pathlib.Path) -> List[TargetSpec]:
    text = path.read_text(encoding="utf-8")
    targets: List[TargetSpec] = []
    for block in parse_blocks(text, "[[target]]"):
        targets.append(
            TargetSpec(
                name=parse_string(block, "name"),
                path=parse_string(block, "path"),
                corpus_dir=parse_string(block, "corpus_dir"),
                generator=parse_string(block, "generator"),
            )
        )
    return targets


def load_cargo_bin_specs(path: pathlib.Path) -> List[CargoBinSpec]:
    text = path.read_text(encoding="utf-8")
    bins: List[CargoBinSpec] = []
    for block in parse_blocks(text, "[[bin]]"):
        bins.append(
            CargoBinSpec(
                name=parse_string(block, "name"),
                path=parse_string(block, "path"),
            )
        )
    return bins


def load_support_file_specs(path: pathlib.Path) -> List[SupportFileSpec]:
    text = path.read_text(encoding="utf-8")
    support_files: List[SupportFileSpec] = []
    for block in parse_blocks(text, "[[support]]"):
        support_files.append(
            SupportFileSpec(
                path=parse_string(block, "path"),
                used_by=parse_string_list(block, "used_by"),
            )
        )
    return support_files


def load_manifest_entries(path: pathlib.Path) -> List[ManifestEntry]:
    text = path.read_text(encoding="utf-8")
    entries: List[ManifestEntry] = []
    for block in parse_blocks(text, "[[entry]]"):
        entries.append(
            ManifestEntry(
                target=parse_string(block, "target"),
                corpus_dir=parse_string(block, "corpus_dir"),
                seed_files=parse_string_list(block, "seed_files"),
                generator=parse_string(block, "generator"),
            )
        )
    return entries


def parse_blocks(text: str, marker: str) -> List[str]:
    blocks: List[str] = []
    current: List[str] = []
    in_block = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped == marker:
            if in_block and current:
                blocks.append("\n".join(current))
            current = []
            in_block = True
            continue
        if in_block and stripped.startswith("[") and stripped.endswith("]"):
            if current:
                blocks.append("\n".join(current))
            current = []
            in_block = False
            continue
        if in_block:
            current.append(line)
    if in_block and current:
        blocks.append("\n".join(current))
    return blocks


def parse_string(block: str, key: str) -> str:
    match = re.search(rf'^\s*{re.escape(key)}\s*=\s*"([^"]*)"', block, re.MULTILINE)
    if not match:
        raise ValueError(f"missing {key} in block:\n{block}")
    return match.group(1)


def parse_string_list(block: str, key: str) -> List[str]:
    match = re.search(rf"^\s*{re.escape(key)}\s*=\s*\[(.*)\]", block, re.MULTILINE)
    if not match:
        raise ValueError(f"missing {key} in block:\n{block}")
    return re.findall(r'"([^"]*)"', match.group(1))


def seed_payloads(target: str) -> Dict[str, bytes]:
    if target == "storage_wal_record_roundtrip":
        payloads = {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8"),
            "seed1": LEGACY_SEED1_PAYLOADS[target],
            "seed-valid-security-audit-frame.bin": wal_record_frame_seed(
                kind_tag=22,
                lsn=1,
                previous_lsn=None,
                transaction_id=None,
                payload=b"security-audit-seed-v1",
            ),
        }
        return payloads
    if target == "btree_node_v1_decode":
        return {
            "seed-basic.bin": btree_node_v1_leaf_seed(),
            "seed-internal.bin": btree_node_v1_internal_seed(),
        }
    if target == "heap_page_v1_decode":
        return {
            "seed-empty-16k.bin": heap_page_v1_empty_seed(16 * 1024),
            "seed-single-32k.bin": heap_page_v1_tuple_seed(32 * 1024, [b"andromeda"]),
            "seed-two-tuples-16k.bin": heap_page_v1_tuple_seed(
                16 * 1024, [b"abc", b"defgh"]
            ),
        }
    if target == "page_codec_v1_decode":
        return {
            "seed-fixed-row-16k.bin": page_codec_v1_seed(1, 1, 16 * 1024, b"fixed-row"),
            "seed-manifest-32k.bin": page_codec_v1_seed(
                3, 2, 32 * 1024, b"manifest-page-v1"
            ),
        }
    if target == "quic_zero_rtt_admission":
        return {
            "seed-basic.bin": bytes(
                [0, 1, 2, 3, 4, 5, 6, 7, 248, 249, 250, 251, 252, 253, 254, 255]
            )
        }
    if target == "durable_audit_journal_decode":
        return durable_audit_journal_decode_seeds()
    payloads = {
        "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8")
    }
    if target in LEGACY_SEED1_PAYLOADS:
        payloads["seed1"] = LEGACY_SEED1_PAYLOADS[target]
    return payloads


def btree_node_v1_leaf_seed() -> bytes:
    header_len = 64
    page_size = 4096
    page = bytearray(page_size)

    def put_u16(offset: int, value: int) -> None:
        page[offset : offset + 2] = value.to_bytes(2, "little")

    def put_u32(offset: int, value: int) -> None:
        page[offset : offset + 4] = value.to_bytes(4, "little")

    def put_u64(offset: int, value: int) -> None:
        page[offset : offset + 8] = value.to_bytes(8, "little")

    put_u32(0, 0x5442_4E41)
    put_u16(4, 1)
    page[6] = 1
    put_u64(8, 42)
    put_u64(16, 7)
    put_u16(24, 0)
    put_u16(26, 0)
    put_u16(28, header_len)
    put_u16(30, page_size)
    put_u64(32, 41)
    put_u64(40, 43)
    put_u16(48, 0)

    crc = header_crc32(page[:header_len])
    put_u32(52, crc if crc != 0 else 1)
    return bytes(page)


def btree_node_v1_internal_seed() -> bytes:
    header_len = 64
    page_size = 4096
    page = bytearray(page_size)
    keys = [b"k10", b"k20"]
    children = [1001, 1002, 1003]
    free_start = header_len + len(children) * 8 + sum(2 + len(key) for key in keys)
    high_key_offset = header_len + len(children) * 8 + 2 + len(keys[0])

    def put_u16(offset: int, value: int) -> None:
        page[offset : offset + 2] = value.to_bytes(2, "little")

    def put_u32(offset: int, value: int) -> None:
        page[offset : offset + 4] = value.to_bytes(4, "little")

    def put_u64(offset: int, value: int) -> None:
        page[offset : offset + 8] = value.to_bytes(8, "little")

    put_u32(0, 0x5442_4E41)
    put_u16(4, 1)
    page[6] = 2
    put_u64(8, 84)
    put_u64(16, 9)
    put_u16(24, len(keys))
    put_u16(26, len(children))
    put_u16(28, free_start)
    put_u16(30, page_size)
    put_u16(48, high_key_offset)

    crc = header_crc32(page[:header_len])
    put_u32(52, crc if crc != 0 else 1)

    offset = header_len
    for child in children:
        put_u64(offset, child)
        offset += 8
    for key in keys:
        put_u16(offset, len(key))
        offset += 2
        page[offset : offset + len(key)] = key
        offset += len(key)
    assert offset == free_start
    return bytes(page)


def header_crc32(header: bytes) -> int:
    state = 0x811C_9DC5
    for idx, byte in enumerate(header):
        if 52 <= idx < 56:
            byte = 0
        state ^= byte
        state = (state * 0x0100_0193) & 0xFFFF_FFFF
    return state


def heap_page_v1_empty_seed(page_size: int) -> bytes:
    return bytes(page_size)


def heap_page_v1_tuple_seed(page_size: int, tuples: List[bytes]) -> bytes:
    header_size = 96
    trailer_size = 48
    slot_entry_size = 5
    metadata_size = 4
    image = bytearray(page_size)
    offset = header_size
    slots = []

    for item in tuples:
        image[offset : offset + len(item)] = item
        slots.append((offset, len(item), 0))
        offset += len(item)

    metadata_offset = page_size - trailer_size - metadata_size
    for slot_id, (slot_offset, slot_len, flags) in enumerate(slots):
        entry_offset = metadata_offset - ((slot_id + 1) * slot_entry_size)
        image[entry_offset : entry_offset + 2] = slot_offset.to_bytes(2, "little")
        image[entry_offset + 2 : entry_offset + 4] = slot_len.to_bytes(2, "little")
        image[entry_offset + 4] = flags

    image[metadata_offset : metadata_offset + 2] = len(slots).to_bytes(2, "little")
    image[metadata_offset + 2 : metadata_offset + 4] = offset.to_bytes(2, "little")
    return bytes(image)


def page_codec_v1_seed(
    page_type_tag: int, page_size_tag: int, page_size: int, payload: bytes
) -> bytes:
    header_len = 112
    trailer_len = 48
    header = bytearray(header_len)
    trailer = bytearray(trailer_len)

    def put_u16(target: bytearray, offset: int, value: int) -> None:
        target[offset : offset + 2] = value.to_bytes(2, "little")

    def put_u32(target: bytearray, offset: int, value: int) -> None:
        target[offset : offset + 4] = value.to_bytes(4, "little")

    def put_u64(target: bytearray, offset: int, value: int) -> None:
        target[offset : offset + 8] = value.to_bytes(8, "little")

    put_u32(header, 0, 0x414E4452)
    put_u16(header, 4, 1)
    put_u16(header, 6, page_size_tag)
    put_u16(header, 8, page_type_tag)
    put_u16(header, 10, 0)
    put_u64(header, 12, 11)
    put_u64(header, 20, 22)
    put_u64(header, 28, 33)
    put_u64(header, 36, 44)
    put_u64(header, 44, 1)
    put_u16(header, 68, header_len)
    put_u32(header, 72, header_len)
    put_u32(header, 76, len(payload))
    put_u32(header, 80, header_len)
    put_u32(header, 84, header_len + len(payload))
    put_u32(header, 88, len(payload))
    put_u16(header, 92, 1)
    put_u32(header, 96, 1)
    put_u32(header, 100, 0x01020304)

    payload_crc = payload_crc64(payload)
    put_u64(trailer, 0, payload_crc)
    trailer[8:40] = bytes([0x5A]) * 32
    put_u64(trailer, 40, 0xA5A5_A5A5_A5A5_A5A5)
    assert header_len + len(payload) + trailer_len <= page_size
    return bytes(header) + payload + bytes(trailer)


def payload_crc64(payload: bytes) -> int:
    state = 0xCBF2_9CE4_8422_2325
    for byte in payload:
        state ^= byte
        state = (state * 0x0000_0100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    return state if state != 0 else 1


def wal_record_frame_seed(
    kind_tag: int,
    lsn: int,
    previous_lsn: int | None,
    transaction_id: int | None,
    payload: bytes,
) -> bytes:
    magic = 0x414E_4452_4F57_414C
    format_version = 1
    header_len = 72
    flags = 0
    if previous_lsn is not None:
        flags |= 0x0001
    if transaction_id is not None:
        flags |= 0x0002

    previous_lsn_value = previous_lsn or 0
    transaction_id_value = transaction_id or 0
    payload_length = len(payload)
    total_length = header_len + payload_length
    record_checksum = wal_record_checksum(
        kind_tag,
        lsn,
        previous_lsn_value,
        transaction_id_value,
        payload,
    )
    header_without_checksum = b"".join(
        [
            magic.to_bytes(8, "little"),
            format_version.to_bytes(2, "little"),
            header_len.to_bytes(2, "little"),
            total_length.to_bytes(8, "little"),
            kind_tag.to_bytes(2, "little"),
            flags.to_bytes(2, "little"),
            lsn.to_bytes(8, "little"),
            previous_lsn_value.to_bytes(8, "little"),
            transaction_id_value.to_bytes(8, "little"),
            payload_length.to_bytes(8, "little"),
            record_checksum.to_bytes(8, "little"),
        ]
    )
    header_checksum = fnv64_nonzero(header_without_checksum)
    return header_without_checksum + header_checksum.to_bytes(8, "little") + payload


def wal_record_checksum(
    kind_tag: int,
    lsn: int,
    previous_lsn: int,
    transaction_id: int,
    payload: bytes,
) -> int:
    fields = [
        kind_tag.to_bytes(8, "little"),
        lsn.to_bytes(8, "little"),
        previous_lsn.to_bytes(8, "little"),
        transaction_id.to_bytes(8, "little"),
        len(payload).to_bytes(8, "little"),
    ]
    return fnv64_nonzero(b"".join(fields) + payload)


def fnv64_nonzero(payload: bytes) -> int:
    state = 0xCBF2_9CE4_8422_2325
    for byte in payload:
        state ^= byte
        state = (state * 0x0000_0100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    return state if state != 0 else 1


def durable_audit_journal_decode_seeds() -> Dict[str, bytes]:
    journal, anchor = durable_audit_valid_journal_and_anchor()
    split = len(journal)
    return {
        "seed-empty-journal.bin": bytes([0]),
        "seed-hostile-prefix.bin": b"\x00andromeda-durable-audit-v2|record_lsn=1\n",
        "seed-valid-generic-audit.bin": bytes([1])
        + split.to_bytes(2, "little")
        + journal
        + anchor,
    }


def durable_audit_valid_journal_and_anchor() -> tuple[bytes, bytes]:
    payload = (
        "andromeda-durable-audit-v2"
        "|record_lsn=1"
        "|durable_lsn=1"
        "|event_id=1"
        "|trace_id=1"
        "|family=GenericAudit"
        "|sequence=1"
        "|retention=ForensicHold"
        "|replay=ForensicOnly"
        "|principal_id=66757a7a2d7072696e636970616c"
        "|certificate_fingerprint=-"
        "|surface=-"
        "|permission=-"
        "|policy_version=-"
        "|policy_digest=-"
        "|request_id=-"
        "|session_id=-"
        "|event_kind=46757a7a47656e657269634175646974"
    )
    checksum = durable_audit_checksum64(payload.encode("ascii"))
    chain_checksum = durable_audit_checksum64(
        f"{0:016x}|{checksum:016x}|{payload}".encode("ascii")
    )
    journal = (
        f"{payload}|previous_chain_checksum={0:016x}"
        f"|chain_checksum={chain_checksum:016x}|checksum={checksum:016x}\n"
    )
    anchor_payload = (
        "andromeda-durable-audit-chain-v1"
        "|first_record_lsn=1"
        "|last_record_lsn=1"
        "|record_count=1"
        f"|tail_chain_checksum={chain_checksum:016x}"
    )
    anchor_checksum = durable_audit_checksum64(anchor_payload.encode("ascii"))
    anchor = f"{anchor_payload}|checksum={anchor_checksum:016x}\n"
    return journal.encode("ascii"), anchor.encode("ascii")


def durable_audit_checksum64(payload: bytes) -> int:
    digest = hashlib.sha256(payload).digest()
    value = int.from_bytes(digest[:8], "big")
    return value if value != 0 else 1


def ensure_seed(root: pathlib.Path, target: str) -> List[pathlib.Path]:
    corpus_dir = root / "fuzz" / "corpus" / target
    corpus_dir.mkdir(parents=True, exist_ok=True)
    written = []
    for name, payload in seed_payloads(target).items():
        seed_file = corpus_dir / name
        if not seed_file.exists() or seed_file.read_bytes() != payload:
            seed_file.write_bytes(payload)
        written.append(seed_file)
    return written


def check_seed_corpus(root: pathlib.Path, targets_file: str) -> int:
    targets_path = root / targets_file
    target_specs = load_target_specs(targets_path)
    support_specs = load_support_file_specs(targets_path)
    manifest_path = root / "fuzz" / "corpus" / "manifest.toml"
    manifest_entries = load_manifest_entries(manifest_path)
    cargo_bin_specs = load_cargo_bin_specs(root / "fuzz" / "Cargo.toml")
    manifest_by_target = {entry.target: entry for entry in manifest_entries}
    target_names = [target.name for target in target_specs]
    support_paths = [support.path for support in support_specs]
    errors: List[str] = []

    target_headers = load_top_level_strings(targets_path)
    if target_headers.get("schema_version") != TARGETS_SCHEMA_VERSION:
        errors.append(
            f"{targets_file} schema_version {target_headers.get('schema_version')!r} "
            f"does not match {TARGETS_SCHEMA_VERSION!r}"
        )

    manifest_headers = load_top_level_strings(manifest_path)
    if manifest_headers.get("schema_version") != CORPUS_SCHEMA_VERSION:
        errors.append(
            "fuzz/corpus/manifest.toml schema_version "
            f"{manifest_headers.get('schema_version')!r} does not match "
            f"{CORPUS_SCHEMA_VERSION!r}"
        )
    if manifest_headers.get("generated_by") != GENERATOR_PATH:
        errors.append(
            "fuzz/corpus/manifest.toml generated_by "
            f"{manifest_headers.get('generated_by')!r} does not match {GENERATOR_PATH!r}"
        )

    if len(manifest_by_target) != len(manifest_entries):
        errors.append("manifest contains duplicate target entries")
    if len(set(target_names)) != len(target_names):
        errors.append(f"{targets_file} contains duplicate target names")
    if len(set(support_paths)) != len(support_paths):
        errors.append(f"{targets_file} contains duplicate support file paths")
    target_file_path_set = {spec.path for spec in target_specs}
    support_path_set = set(support_paths)
    overlapping_paths = sorted(target_file_path_set & support_path_set)
    if overlapping_paths:
        errors.append(
            f"{targets_file} marks files as both target and support: {overlapping_paths}"
        )
    if len({item.name for item in cargo_bin_specs}) != len(cargo_bin_specs):
        errors.append("fuzz/Cargo.toml contains duplicate [[bin]] names")
    if len({item.path for item in cargo_bin_specs}) != len(cargo_bin_specs):
        errors.append("fuzz/Cargo.toml contains duplicate [[bin]] paths")

    actual_dirs = sorted(
        path.name for path in (root / "fuzz" / "corpus").iterdir() if path.is_dir()
    )
    expected_dirs = sorted(target_names)
    if actual_dirs != expected_dirs:
        errors.append(
            "corpus directory set mismatch: "
            f"actual={actual_dirs} expected={expected_dirs}"
        )

    cargo_by_name = {item.name: item for item in cargo_bin_specs}
    cargo_names = sorted(cargo_by_name)
    cargo_paths = {item.path for item in cargo_bin_specs}
    if cargo_names != expected_dirs:
        errors.append(
            f"fuzz/Cargo.toml target set mismatch: actual={cargo_names} expected={expected_dirs}"
        )
    support_cargo_paths = sorted(support_path_set & cargo_paths)
    if support_cargo_paths:
        errors.append(
            "support files must not be executable fuzz bins: "
            f"{support_cargo_paths}"
        )

    target_file_paths = sorted(
        path.relative_to(root / "fuzz").as_posix()
        for path in (root / "fuzz" / "fuzz_targets").glob("*.rs")
    )
    expected_target_file_paths = sorted(spec.path for spec in target_specs)
    expected_source_file_paths = sorted(expected_target_file_paths + support_paths)
    if target_file_paths != expected_source_file_paths:
        errors.append(
            "fuzz source file set mismatch: "
            f"actual={target_file_paths} expected={expected_source_file_paths}"
        )

    for spec in target_specs:
        cargo_bin = cargo_by_name.get(spec.name)
        if cargo_bin is None:
            errors.append(f"{spec.name}: missing fuzz/Cargo.toml [[bin]] entry")
        elif cargo_bin.path != spec.path:
            errors.append(
                f"{spec.name}: Cargo.toml path {cargo_bin.path!r} "
                f"does not match targets.toml {spec.path!r}"
            )

        target_path = root / "fuzz" / spec.path
        if not target_path.is_file():
            errors.append(f"{spec.name}: target path missing: {target_path.as_posix()}")

        corpus_dir = root / spec.corpus_dir
        if not corpus_dir.is_dir():
            errors.append(f"{spec.name}: corpus directory missing: {spec.corpus_dir}")
            continue

        entry = manifest_by_target.get(spec.name)
        if entry is None:
            errors.append(f"{spec.name}: missing manifest entry")
            continue
        if entry.corpus_dir != spec.corpus_dir:
            errors.append(
                f"{spec.name}: manifest corpus_dir {entry.corpus_dir!r} "
                f"does not match targets.toml {spec.corpus_dir!r}"
            )

        actual_seed_files = sorted(path.name for path in corpus_dir.iterdir() if path.is_file())
        manifest_seed_files = sorted(entry.seed_files)
        if actual_seed_files != manifest_seed_files:
            errors.append(
                f"{spec.name}: manifest seed_files {manifest_seed_files} "
                f"do not match actual files {actual_seed_files}"
            )

        expected_payloads = seed_payloads(spec.name)
        generated_seed_files = sorted(expected_payloads)
        if manifest_seed_files != generated_seed_files:
            errors.append(
                f"{spec.name}: manifest seed_files {manifest_seed_files} "
                f"do not match generated seeds {generated_seed_files}"
            )

        for seed_name, expected_payload in expected_payloads.items():
            if seed_name not in entry.seed_files:
                errors.append(f"{spec.name}: generated seed {seed_name} missing from manifest")
                continue
            seed_path = corpus_dir / seed_name
            if not seed_path.is_file():
                errors.append(f"{spec.name}: generated seed {seed_name} missing on disk")
                continue
            actual_payload = seed_path.read_bytes()
            if actual_payload != expected_payload:
                errors.append(
                    f"{spec.name}: generated seed {seed_name} is not deterministic "
                    f"(actual {len(actual_payload)} bytes, expected {len(expected_payload)} bytes)"
                )

    target_name_set = set(target_names)
    for spec in support_specs:
        support_path = root / "fuzz" / spec.path
        if not support_path.is_file():
            errors.append(f"support file path missing: {support_path.as_posix()}")
        if not spec.path.startswith("fuzz_targets/") or not spec.path.endswith(".rs"):
            errors.append(
                f"support file path must stay under fuzz_targets/*.rs: {spec.path!r}"
            )
        if not spec.used_by:
            errors.append(f"{spec.path}: support file must declare at least one used_by target")
        unknown_targets = sorted(set(spec.used_by) - target_name_set)
        if unknown_targets:
            errors.append(f"{spec.path}: unknown used_by targets {unknown_targets}")

    manifest_targets = sorted(manifest_by_target)
    if manifest_targets != expected_dirs:
        errors.append(
            f"manifest target set mismatch: actual={manifest_targets} expected={expected_dirs}"
        )

    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1

    print(
        f"seed corpus check passed: {len(target_specs)} targets, "
        f"{sum(len(entry.seed_files) for entry in manifest_entries)} manifest seeds"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate deterministic fuzz seed corpus")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--ensure-only", action="store_true")
    parser.add_argument("--targets-file", default="fuzz/targets.toml")
    args = parser.parse_args()

    root = pathlib.Path.cwd()

    if args.check:
        return check_seed_corpus(root, args.targets_file)

    targets = load_targets(root / args.targets_file)
    if not targets:
        raise SystemExit("No targets found in fuzz/targets.toml")

    created = []
    for target in targets:
        created.extend(ensure_seed(root, target))

    if not args.ensure_only:
        for item in created:
            print(item.as_posix())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
