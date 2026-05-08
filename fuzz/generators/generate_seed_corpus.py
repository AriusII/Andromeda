#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import pathlib
import re
import tomllib
from typing import Any, Dict, List, Optional, Tuple


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
DEFAULT_TARGETS_FILE = "tests/fuzzing/targets.toml"
CORPUS_MANIFEST_PATH = "tests/fuzzing/corpus/manifest.toml"

HEAP_PAGE_V1_PAYLOAD_OFFSET = 112
HEAP_PAGE_V1_TRAILER_SIZE = 48
HEAP_PAGE_V1_SLOT_METADATA_SIZE = 4
HEAP_PAGE_V1_SLOT_ENTRY_SIZE = 5
HEAP_PAGE_V1_DELETED_SLOT_FLAG = 0x01
PAGE_CODEC_V1_MAGIC = 0x414E_4452
PAGE_CODEC_V1_FORMAT_VERSION = 1
PAGE_CODEC_V1_HEADER_LEN = 112

LEGACY_SEED1_PAYLOADS: Dict[str, bytes] = {
    "frame_codec_no_panic": b"frame\n",
    "result_sequence_state_machine": b"result-sequence\n",
    "proto_frame_envelope_decode": b"proto-envelope\n",
    "proto_rpc_completion_decode": b"completion\n",
    "srpl_parser_signature_decode": b"PROC demo(a:int)->rows\n",
    "storage_wal_record_roundtrip": b"wal\n",
    "wal_record_roundtrip": b"wal\n",
}


def load_targets(path: pathlib.Path) -> List[str]:
    return [target.name for target in load_target_specs(path)]


def load_toml(path: pathlib.Path) -> Dict[str, Any]:
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    if not isinstance(data, dict):
        raise ValueError(f"{path.as_posix()}: TOML root must be a table")
    return data


def require_table_list(
    data: Dict[str, Any], key: str, *, path: pathlib.Path
) -> List[Dict[str, Any]]:
    raw_items = data.get(key, [])
    if not isinstance(raw_items, list):
        raise ValueError(f"{path.as_posix()}: {key!r} must be an array of tables")
    items: List[Dict[str, Any]] = []
    for index, raw in enumerate(raw_items, start=1):
        if not isinstance(raw, dict):
            raise ValueError(
                f"{path.as_posix()} [[{key}]] #{index}: entry must be a table"
            )
        items.append(raw)
    return items


def require_string(table: Dict[str, Any], key: str, *, context: str) -> str:
    value = table.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{context}: {key} must be a non-empty string")
    return value


def require_string_list(table: Dict[str, Any], key: str, *, context: str) -> List[str]:
    value = table.get(key)
    if not isinstance(value, list):
        raise ValueError(f"{context}: {key} must be a string array")
    items: List[str] = []
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item:
            raise ValueError(f"{context}: {key}[{index}] must be a non-empty string")
        items.append(item)
    return items


def load_top_level_strings(path: pathlib.Path) -> Dict[str, str]:
    data = load_toml(path)
    return {key: value for key, value in data.items() if isinstance(value, str)}


def load_target_specs(path: pathlib.Path) -> List[TargetSpec]:
    data = load_toml(path)
    targets: List[TargetSpec] = []
    for index, table in enumerate(require_table_list(data, "target", path=path), start=1):
        context = f"{path.as_posix()} [[target]] #{index}"
        targets.append(
            TargetSpec(
                name=require_string(table, "name", context=context),
                path=require_string(table, "path", context=context),
                corpus_dir=require_string(table, "corpus_dir", context=context),
                generator=require_string(table, "generator", context=context),
            )
        )
    return targets


def load_cargo_bin_specs(path: pathlib.Path) -> List[CargoBinSpec]:
    data = load_toml(path)
    bins: List[CargoBinSpec] = []
    for index, table in enumerate(require_table_list(data, "bin", path=path), start=1):
        context = f"{path.as_posix()} [[bin]] #{index}"
        bins.append(
            CargoBinSpec(
                name=require_string(table, "name", context=context),
                path=require_string(table, "path", context=context),
            )
        )
    return bins


def load_support_file_specs(path: pathlib.Path) -> List[SupportFileSpec]:
    data = load_toml(path)
    support_files: List[SupportFileSpec] = []
    for index, table in enumerate(require_table_list(data, "support", path=path), start=1):
        context = f"{path.as_posix()} [[support]] #{index}"
        support_files.append(
            SupportFileSpec(
                path=require_string(table, "path", context=context),
                used_by=require_string_list(table, "used_by", context=context),
            )
        )
    return support_files


def load_manifest_entries(path: pathlib.Path) -> List[ManifestEntry]:
    data = load_toml(path)
    entries: List[ManifestEntry] = []
    for index, table in enumerate(require_table_list(data, "entry", path=path), start=1):
        context = f"{path.as_posix()} [[entry]] #{index}"
        entries.append(
            ManifestEntry(
                target=require_string(table, "target", context=context),
                corpus_dir=require_string(table, "corpus_dir", context=context),
                seed_files=require_string_list(table, "seed_files", context=context),
                generator=require_string(table, "generator", context=context),
            )
        )
    return entries


def seed_payloads(target: str) -> Dict[str, bytes]:
    if target == "srpl_parser_owner_decode":
        return seed_payloads("srpl_parser_signature_decode")
    if target in {"storage_wal_record_roundtrip", "wal_record_roundtrip"}:
        seed_identity = "storage_wal_record_roundtrip"
        payloads = {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{seed_identity}::v1".encode(
                "utf-8"
            ),
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
            "seed-deleted-compacted-parity.bin": heap_page_v1_deleted_compacted_parity_seed(),
            "seed-free-offset-crosses-slot-directory.bin": (
                heap_page_v1_free_offset_crosses_slot_directory_seed()
            ),
            "seed-header-footer-slot-count-mismatch.bin": (
                heap_page_v1_header_footer_slot_count_mismatch_seed()
            ),
            "seed-overlapping-live-tuples.bin": heap_page_v1_overlapping_live_tuple_ranges_seed(),
            "seed-slot-entry-outside-payload-area.bin": (
                heap_page_v1_slot_entry_outside_payload_area_seed()
            ),
            "seed-single-32k.bin": heap_page_v1_tuple_seed(32 * 1024, [b"andromeda"]),
            "seed-sparse-deleted-middle-live-higher.bin": (
                heap_page_v1_sparse_deleted_middle_live_higher_seed()
            ),
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
    if target == "proto_rpc_execute_request_decode":
        return {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8"),
            "seed-valid-reserve-stock-execute.bin": proto_rpc_execute_request_seed(),
        }
    if target == "proto_invocation_response_sequence_decode":
        return {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8"),
            "seed-valid-metadata-batch-completion.bin": (
                proto_invocation_response_sequence_seed()
            ),
        }
    if target == "quic_zero_rtt_admission":
        return {
            "seed-basic.bin": bytes(
                [0, 1, 2, 3, 4, 5, 6, 7, 248, 249, 250, 251, 252, 253, 254, 255]
            )
        }
    if target == "quic_typed_frame_envelope_decode":
        return {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8"),
            "seed-valid-rpc-execute-envelope-frame.bin": quic_typed_frame_envelope_seed(),
        }
    if target == "durable_audit_journal_decode":
        return durable_audit_journal_decode_seeds()
    if target == "rpc_protocol_frame_codec_decode":
        return {
            "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8"),
            "seed-valid-rpc-execute-frame.bin": rpc_protocol_frame_seed(
                frame_type=5,
                payload=b"rpc-protocol-frame-seed-v1",
            ),
        }
    if target == "security_contract_admission_matrix":
        return {
            "seed-surface-permission-matrix.bin": security_contract_admission_matrix_seed()
        }
    if target == "manifest_boundary":
        return {"seed-basic.bin": manifest_boundary_seed()}
    if target == "recovery_manifest_durability_boundary":
        return {"seed-basic.bin": recovery_manifest_durability_boundary_seed()}
    payloads = {
        "seed-basic.bin": f"ANDROMEDA-FUZZ-SEED::{target}::v1".encode("utf-8")
    }
    if target in LEGACY_SEED1_PAYLOADS:
        payloads["seed1"] = LEGACY_SEED1_PAYLOADS[target]
    return payloads


def manifest_boundary_seed() -> bytes:
    payload = bytearray(104)
    payload[0:8] = (100).to_bytes(8, "little")
    payload[8:16] = (120).to_bytes(8, "little")
    payload[16:24] = (140).to_bytes(8, "little")
    payload[24:32] = (140).to_bytes(8, "little")
    payload[32:40] = (140).to_bytes(8, "little")
    payload[40:48] = (1).to_bytes(8, "little")
    payload[48:56] = (2).to_bytes(8, "little")
    payload[56:64] = (3).to_bytes(8, "little")
    payload[64:68] = (0xA1B2C3D4).to_bytes(4, "little")
    payload[68:100] = bytes(range(32))
    return bytes(payload)


def recovery_manifest_durability_boundary_seed() -> bytes:
    payload = bytearray(76)
    payload[0:8] = (1).to_bytes(8, "little")
    payload[8:16] = (2).to_bytes(8, "little")
    payload[16:24] = (3).to_bytes(8, "little")
    payload[24:32] = (100).to_bytes(8, "little")
    payload[32:40] = (120).to_bytes(8, "little")
    payload[40:44] = (0x01020304).to_bytes(4, "little")
    payload[44:76] = bytes(reversed(range(32)))
    return bytes(payload)


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
    image = bytearray(page_size)
    offset = HEAP_PAGE_V1_PAYLOAD_OFFSET
    slots = []

    for item in tuples:
        image[offset : offset + len(item)] = item
        slots.append((offset, len(item), 0))
        offset += len(item)

    return heap_page_v1_image(page_size, slots, offset, image)


def heap_page_v1_deleted_compacted_parity_seed() -> bytes:
    image = bytearray(16 * 1024)
    first = b"live"
    image[HEAP_PAGE_V1_PAYLOAD_OFFSET : HEAP_PAGE_V1_PAYLOAD_OFFSET + len(first)] = first
    return heap_page_v1_image(
        16 * 1024,
        [
            (HEAP_PAGE_V1_PAYLOAD_OFFSET, len(first), 0),
            (0, 8, HEAP_PAGE_V1_DELETED_SLOT_FLAG),
        ],
        HEAP_PAGE_V1_PAYLOAD_OFFSET + len(first),
        image,
    )


def heap_page_v1_sparse_deleted_middle_live_higher_seed() -> bytes:
    image = bytearray(16 * 1024)
    first = b"alpha"
    second = b"omega!"
    first_offset = HEAP_PAGE_V1_PAYLOAD_OFFSET
    second_offset = first_offset + len(first)
    image[first_offset : first_offset + len(first)] = first
    image[second_offset : second_offset + len(second)] = second
    return heap_page_v1_image(
        16 * 1024,
        [
            (first_offset, len(first), 0),
            (0, 6, HEAP_PAGE_V1_DELETED_SLOT_FLAG),
            (second_offset, len(second), 0),
        ],
        second_offset + len(second),
        image,
    )


def heap_page_v1_header_footer_slot_count_mismatch_seed() -> bytes:
    image = bytearray(16 * 1024)
    payload = b"hdrf"
    image[HEAP_PAGE_V1_PAYLOAD_OFFSET : HEAP_PAGE_V1_PAYLOAD_OFFSET + len(payload)] = payload
    return heap_page_v1_image(
        16 * 1024,
        [(HEAP_PAGE_V1_PAYLOAD_OFFSET, len(payload), 0)],
        HEAP_PAGE_V1_PAYLOAD_OFFSET + len(payload),
        image,
        header_slot_count=2,
    )


def heap_page_v1_free_offset_crosses_slot_directory_seed() -> bytes:
    page_size = 16 * 1024
    image = bytearray(page_size)
    payload = b"free"
    image[HEAP_PAGE_V1_PAYLOAD_OFFSET : HEAP_PAGE_V1_PAYLOAD_OFFSET + len(payload)] = payload
    slot_base = heap_page_v1_metadata_offset(page_size) - HEAP_PAGE_V1_SLOT_ENTRY_SIZE
    return heap_page_v1_image(
        page_size,
        [(HEAP_PAGE_V1_PAYLOAD_OFFSET, len(payload), 0)],
        slot_base + 1,
        image,
    )


def heap_page_v1_slot_entry_outside_payload_area_seed() -> bytes:
    image = bytearray(16 * 1024)
    legacy_offset = 96
    payload = b"old!"
    image[legacy_offset : legacy_offset + len(payload)] = payload
    return heap_page_v1_image(
        16 * 1024,
        [(legacy_offset, len(payload), 0)],
        HEAP_PAGE_V1_PAYLOAD_OFFSET + len(payload),
        image,
    )


def heap_page_v1_overlapping_live_tuple_ranges_seed() -> bytes:
    image = bytearray(16 * 1024)
    first = b"abcdefgh"
    second = b"WXYZ1234"
    first_offset = HEAP_PAGE_V1_PAYLOAD_OFFSET
    second_offset = first_offset + 4
    image[first_offset : first_offset + len(first)] = first
    image[second_offset : second_offset + len(second)] = second
    return heap_page_v1_image(
        16 * 1024,
        [
            (first_offset, len(first), 0),
            (second_offset, len(second), 0),
        ],
        second_offset + len(second),
        image,
    )


def heap_page_v1_image(
    page_size: int,
    slots: List[Tuple[int, int, int]],
    free_offset: int,
    image: bytearray,
    *,
    header_slot_count: Optional[int] = None,
) -> bytes:
    metadata_offset = heap_page_v1_metadata_offset(page_size)

    if header_slot_count is not None:
        heap_page_v1_write_page_codec_header(image, header_slot_count)

    for slot_id, (slot_offset, slot_len, flags) in enumerate(slots):
        entry_offset = metadata_offset - ((slot_id + 1) * HEAP_PAGE_V1_SLOT_ENTRY_SIZE)
        put_u16(image, entry_offset, slot_offset)
        put_u16(image, entry_offset + 2, slot_len)
        image[entry_offset + 4] = flags

    put_u16(image, metadata_offset, len(slots))
    put_u16(image, metadata_offset + 2, free_offset)
    return bytes(image)


def heap_page_v1_metadata_offset(page_size: int) -> int:
    return page_size - HEAP_PAGE_V1_TRAILER_SIZE - HEAP_PAGE_V1_SLOT_METADATA_SIZE


def heap_page_v1_write_page_codec_header(image: bytearray, slot_count: int) -> None:
    put_u32(image, 0, PAGE_CODEC_V1_MAGIC)
    put_u16(image, 4, PAGE_CODEC_V1_FORMAT_VERSION)
    put_u16(image, 68, PAGE_CODEC_V1_HEADER_LEN)
    put_u32(image, 72, HEAP_PAGE_V1_PAYLOAD_OFFSET)
    put_u16(image, 92, slot_count)


def put_u16(target: bytearray, offset: int, value: int) -> None:
    target[offset : offset + 2] = value.to_bytes(2, "little")


def put_u32(target: bytearray, offset: int, value: int) -> None:
    target[offset : offset + 4] = value.to_bytes(4, "little")


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
PROTO_WIRE_VARINT = 0
PROTO_WIRE_LEN = 2


def proto_rpc_execute_request_seed() -> bytes:
    argument = b"".join(
        [
            proto_string_field(1, "Quantity"),
            proto_string_field(2, "i64"),
            proto_bytes_field(3, (3).to_bytes(8, "little")),
        ]
    )
    budget = b"".join(
        [
            proto_varint_field(1, 5_000),
            proto_varint_field(2, 64 * 1024),
            proto_varint_field(3, 128 * 1024),
            proto_varint_field(4, 1),
        ]
    )
    return b"".join(
        [
            proto_string_field(1, "Inventory.ReserveStock"),
            proto_bytes_field(2, bytes([9]) * 32),
            proto_varint_field(3, 44),
            proto_string_field(4, "application"),
            proto_message_field(5, argument),
            proto_message_field(6, budget),
            proto_varint_field(7, 6),
        ]
    )


def proto_invocation_response_sequence_seed() -> bytes:
    correlation = proto_invocation_correlation_seed()
    responses = [
        proto_invocation_response_seed(
            correlation=correlation,
            response_index=0,
            response_field=3,
            response_payload=proto_rpc_metadata_seed(),
        ),
        proto_invocation_response_seed(
            correlation=correlation,
            response_index=1,
            response_field=4,
            response_payload=proto_rpc_batch_seed(),
        ),
        proto_invocation_response_seed(
            correlation=correlation,
            response_index=2,
            response_field=5,
            response_payload=proto_rpc_completion_seed(),
        ),
    ]
    framed = bytearray([1])
    for response in responses:
        framed.extend(len(response).to_bytes(2, "little"))
        framed.extend(response)
    return bytes(framed)


def proto_invocation_correlation_seed() -> bytes:
    return b"".join(
        [
            proto_varint_field(1, 101),
            proto_varint_field(2, 202),
            proto_string_field(3, "trace-proto-101"),
            proto_bytes_field(4, bytes([9]) * 32),
            proto_varint_field(5, 44),
            proto_varint_field(6, 303),
            proto_varint_field(7, 6),
            proto_varint_field(8, 11),
        ]
    )


def proto_invocation_response_seed(
    *, correlation: bytes, response_index: int, response_field: int, response_payload: bytes
) -> bytes:
    return b"".join(
        [
            proto_message_field(1, correlation),
            proto_varint_field(2, response_index),
            proto_message_field(response_field, response_payload),
        ]
    )


def proto_rpc_metadata_seed() -> bytes:
    policy = b"".join(
        [
            proto_varint_field(1, 1),
            proto_string_field(2, "reservation row required"),
        ]
    )
    return b"".join(
        [
            proto_message_field(1, proto_result_stream_descriptor_seed()),
            proto_message_field(2, policy),
        ]
    )


def proto_result_stream_descriptor_seed() -> bytes:
    column = b"".join(
        [
            proto_string_field(1, "reservation_id"),
            proto_varint_field(2, 0),
            proto_string_field(3, "u64"),
        ]
    )
    return b"".join(
        [
            proto_string_field(1, "Inventory.ReserveStock.Reservation"),
            proto_message_field(2, column),
            proto_varint_field(3, 4),
            proto_varint_field(4, 3),
            proto_varint_field(5, 1),
            proto_varint_field(6, 1),
        ]
    )


def proto_rpc_batch_seed() -> bytes:
    return b"".join(
        [
            proto_string_field(1, "Inventory.ReserveStock.Reservation"),
            proto_varint_field(2, 0),
            proto_varint_field(3, 1),
            proto_bytes_field(4, b"\x01"),
            proto_varint_field(5, 1),
            proto_varint_field(6, 1),
        ]
    )


def proto_rpc_completion_seed() -> bytes:
    summary = b"".join(
        [
            proto_string_field(1, "Inventory.ReserveStock.Reservation"),
            proto_varint_field(2, 1),
            proto_varint_field(3, 1),
        ]
    )
    return b"".join(
        [
            proto_varint_field(1, 1),
            proto_varint_field(2, 1),
            proto_varint_field(3, 404),
            proto_varint_field(32, 101),
            proto_varint_field(33, 202),
            proto_string_field(34, "trace-proto-101"),
            proto_varint_field(35, 2),
            proto_varint_field(36, 505),
            proto_message_field(37, summary),
        ]
    )


def quic_typed_frame_envelope_seed() -> bytes:
    envelope = proto_frame_envelope_seed(
        payload_kind=5,
        payload=proto_rpc_execute_request_seed(),
    )
    return rpc_protocol_frame_seed(frame_type=5, payload=envelope)


def proto_frame_envelope_seed(payload_kind: int, payload: bytes) -> bytes:
    version = b"".join([proto_varint_field(1, 1), proto_varint_field(2, 0)])
    return b"".join(
        [
            proto_message_field(1, version),
            proto_bytes_field(2, bytes([9]) * 32),
            proto_varint_field(3, 44),
            proto_varint_field(4, 501),
            proto_varint_field(5, 601),
            proto_varint_field(6, 701),
            proto_varint_field(7, payload_kind),
            proto_bytes_field(8, payload),
        ]
    )


def proto_varint_field(field_number: int, value: int) -> bytes:
    return proto_key(field_number, PROTO_WIRE_VARINT) + proto_varint(value)


def proto_string_field(field_number: int, value: str) -> bytes:
    return proto_bytes_field(field_number, value.encode("utf-8"))


def proto_message_field(field_number: int, payload: bytes) -> bytes:
    return proto_bytes_field(field_number, payload)


def proto_bytes_field(field_number: int, payload: bytes) -> bytes:
    return proto_key(field_number, PROTO_WIRE_LEN) + proto_varint(len(payload)) + payload


def proto_key(field_number: int, wire_type: int) -> bytes:
    return proto_varint((field_number << 3) | wire_type)


def proto_varint(value: int) -> bytes:
    encoded = bytearray()
    while value >= 0x80:
        encoded.append((value & 0x7F) | 0x80)
        value >>= 7
    encoded.append(value)
    return bytes(encoded)


def rpc_protocol_frame_seed(
    *, frame_type: int, payload: bytes, request_id: int = 501, session_id: int = 601
) -> bytes:
    tx_id = 701
    header = bytearray()
    header.extend((52).to_bytes(2, "big"))
    header.extend((1).to_bytes(2, "big"))
    header.extend(frame_type.to_bytes(4, "big"))
    header.extend(request_id.to_bytes(8, "big"))
    header.extend(session_id.to_bytes(8, "big"))
    header.extend(tx_id.to_bytes(8, "big"))
    header.append(1)
    header.extend(b"\x00\x00\x00")
    header.extend(len(payload).to_bytes(8, "big"))
    header.extend((0).to_bytes(4, "big"))
    header.extend((0).to_bytes(4, "big"))
    crc = rpc_frame_crc32(header)
    header[48:52] = crc.to_bytes(4, "big")
    return bytes(header) + payload


def rpc_frame_crc32(payload: bytes) -> int:
    state = 0xFFFF_FFFF
    for byte in payload:
        state ^= byte
        for _ in range(8):
            mask = (-(state & 1)) & 0xFFFF_FFFF
            state = ((state >> 1) ^ (0xEDB8_8320 & mask)) & 0xFFFF_FFFF
    return (~state) & 0xFFFF_FFFF


def security_contract_admission_matrix_seed() -> bytes:
    return bytes([1, 0, 0, *range(3, 64), 128, 129, 250, 251, 252, 253, 254, 255])


def durable_audit_security_decision_journal_and_anchor() -> tuple[bytes, bytes]:
    payload = durable_audit_record_payload(
        record_lsn=1,
        durable_lsn=1,
        event_id=2,
        trace_id=2,
        family="SecurityDecision",
        retention="SecurityPolicy",
        replay="ForensicOnly",
        principal_id="fuzz-principal",
        certificate_fingerprint="sha256:fuzz-cert",
        surface="Application",
        permission="ExecuteProcedure",
        policy_version="1",
        policy_digest="security-policy-v1",
        request_id="501",
        session_id="601",
        event_kind="FuzzSecurityDecision",
    )
    return durable_audit_journal_and_anchor([payload])


def durable_audit_admission_decision_journal_and_anchor() -> tuple[bytes, bytes]:
    payload = durable_audit_record_payload(
        record_lsn=1,
        durable_lsn=1,
        event_id=3,
        trace_id=3,
        family="AdmissionDecision",
        retention="ForensicHold",
        replay="ForensicOnly",
        principal_id="fuzz-principal",
        certificate_fingerprint=None,
        surface=None,
        permission=None,
        policy_version=None,
        policy_digest=None,
        request_id=None,
        session_id=None,
        event_kind="FuzzAdmissionDecision",
    )
    return durable_audit_journal_and_anchor([payload])


def durable_audit_record_payload(
    *,
    record_lsn: int,
    durable_lsn: int,
    event_id: int,
    trace_id: int,
    family: str,
    retention: str,
    replay: str,
    principal_id: str,
    certificate_fingerprint: Optional[str],
    surface: Optional[str],
    permission: Optional[str],
    policy_version: Optional[str],
    policy_digest: Optional[str],
    request_id: Optional[str],
    session_id: Optional[str],
    event_kind: str,
) -> str:
    return (
        "andromeda-durable-audit-v2"
        f"|record_lsn={record_lsn}"
        f"|durable_lsn={durable_lsn}"
        f"|event_id={event_id}"
        f"|trace_id={trace_id}"
        f"|family={family}"
        "|sequence=1"
        f"|retention={retention}"
        f"|replay={replay}"
        f"|principal_id={durable_audit_hex(principal_id)}"
        f"|certificate_fingerprint={durable_audit_optional_hex(certificate_fingerprint)}"
        f"|surface={durable_audit_optional_raw(surface)}"
        f"|permission={durable_audit_optional_raw(permission)}"
        f"|policy_version={durable_audit_optional_raw(policy_version)}"
        f"|policy_digest={durable_audit_optional_hex(policy_digest)}"
        f"|request_id={durable_audit_optional_raw(request_id)}"
        f"|session_id={durable_audit_optional_raw(session_id)}"
        f"|event_kind={durable_audit_hex(event_kind)}"
    )


def durable_audit_journal_and_anchor(payloads: List[str]) -> tuple[bytes, bytes]:
    previous_chain_checksum = 0
    lines = []
    record_lsns = []
    for payload in payloads:
        line, previous_chain_checksum = durable_audit_journal_line(
            payload,
            previous_chain_checksum,
        )
        lines.append(line)
        match = re.search(r"\|record_lsn=(\d+)", payload)
        if match:
            record_lsns.append(int(match.group(1)))

    first_record_lsn = min(record_lsns) if record_lsns else 1
    last_record_lsn = max(record_lsns) if record_lsns else 1
    anchor = durable_audit_anchor(
        first_record_lsn=first_record_lsn,
        last_record_lsn=last_record_lsn,
        record_count=len(lines),
        tail_chain_checksum=previous_chain_checksum,
    )
    return "".join(lines).encode("ascii"), anchor.encode("ascii")


def durable_audit_journal_line(payload: str, previous_chain_checksum: int) -> tuple[str, int]:
    checksum = durable_audit_checksum64(payload.encode("ascii"))
    chain_checksum = durable_audit_checksum64(
        f"{previous_chain_checksum:016x}|{checksum:016x}|{payload}".encode("ascii")
    )
    line = (
        f"{payload}|previous_chain_checksum={previous_chain_checksum:016x}"
        f"|chain_checksum={chain_checksum:016x}|checksum={checksum:016x}\n"
    )
    return line, chain_checksum


def durable_audit_anchor(
    *, first_record_lsn: int, last_record_lsn: int, record_count: int, tail_chain_checksum: int
) -> str:
    anchor_payload = (
        "andromeda-durable-audit-chain-v1"
        f"|first_record_lsn={first_record_lsn}"
        f"|last_record_lsn={last_record_lsn}"
        f"|record_count={record_count}"
        f"|tail_chain_checksum={tail_chain_checksum:016x}"
    )
    anchor_checksum = durable_audit_checksum64(anchor_payload.encode("ascii"))
    return f"{anchor_payload}|checksum={anchor_checksum:016x}\n"


def durable_audit_hex(value: str) -> str:
    return value.encode("utf-8").hex()


def durable_audit_optional_hex(value: Optional[str]) -> str:
    return durable_audit_hex(value) if value is not None else "-"


def durable_audit_optional_raw(value: Optional[str]) -> str:
    return value if value is not None else "-"


def durable_audit_journal_decode_seeds() -> Dict[str, bytes]:
    journal, anchor = durable_audit_valid_journal_and_anchor()
    split = len(journal)
    security_journal, security_anchor = durable_audit_security_decision_journal_and_anchor()
    security_split = len(security_journal)
    admission_journal, admission_anchor = durable_audit_admission_decision_journal_and_anchor()
    admission_split = len(admission_journal)
    return {
        "seed-empty-journal.bin": bytes([0]),
        "seed-hostile-prefix.bin": b"\x00andromeda-durable-audit-v2|record_lsn=1\n",
        "seed-valid-admission-decision.bin": bytes([1])
        + admission_split.to_bytes(2, "little")
        + admission_journal
        + admission_anchor,
        "seed-valid-generic-audit.bin": bytes([1])
        + split.to_bytes(2, "little")
        + journal
        + anchor,
        "seed-valid-security-decision.bin": bytes([1])
        + security_split.to_bytes(2, "little")
        + security_journal
        + security_anchor,
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


def ensure_seed(
    root: pathlib.Path, target: TargetSpec, seed_files: Optional[List[str]] = None
) -> List[pathlib.Path]:
    corpus_dir = root / target.corpus_dir
    corpus_dir.mkdir(parents=True, exist_ok=True)
    payloads = seed_payloads(target.name)
    selected_seed_files = (
        sorted(seed_files) if seed_files is not None else sorted(payloads)
    )
    written = []
    for name in selected_seed_files:
        payload = payloads.get(name)
        if payload is None:
            raise ValueError(
                f"{target.name}: manifest declares unknown deterministic seed {name!r}"
            )
        seed_file = corpus_dir / name
        if not seed_file.exists() or seed_file.read_bytes() != payload:
            seed_file.write_bytes(payload)
        written.append(seed_file)
    return written


def seed_payload_is_deterministic(seed_name: str, actual: bytes, expected: bytes) -> bool:
    if actual == expected:
        return True
    if seed_name == "seed1" and actual == expected.replace(b"\n", b"\r\n"):
        return True
    return False


def write_manifest(root: pathlib.Path, entries: List[ManifestEntry]) -> pathlib.Path:
    manifest_path = root / CORPUS_MANIFEST_PATH
    lines = [
        f'schema_version = "{CORPUS_SCHEMA_VERSION}"',
        f'generated_by = "{GENERATOR_PATH}"',
        "",
    ]
    for entry in entries:
        seed_files = sorted(entry.seed_files)
        seed_list = ", ".join(f'"{name}"' for name in seed_files)
        lines.extend(
            [
                "[[entry]]",
                f'target = "{entry.target}"',
                f'corpus_dir = "{entry.corpus_dir}"',
                f"seed_files = [{seed_list}]",
                f'generator = "{entry.generator}"',
                "",
            ]
        )
    manifest_path.write_text("\n".join(lines), encoding="utf-8")
    return manifest_path


def check_seed_corpus(root: pathlib.Path, targets_file: str) -> int:
    targets_path = root / targets_file
    manifest_path = root / CORPUS_MANIFEST_PATH
    try:
        target_specs = load_target_specs(targets_path)
        support_specs = load_support_file_specs(targets_path)
        manifest_entries = load_manifest_entries(manifest_path)
        cargo_bin_specs = load_cargo_bin_specs(root / "fuzz" / "Cargo.toml")
    except ValueError as error:
        print(f"ERROR: {error}")
        return 1
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
            "tests/fuzzing/corpus/manifest.toml schema_version "
            f"{manifest_headers.get('schema_version')!r} does not match "
            f"{CORPUS_SCHEMA_VERSION!r}"
        )
    if manifest_headers.get("generated_by") != GENERATOR_PATH:
        errors.append(
            "tests/fuzzing/corpus/manifest.toml generated_by "
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

    actual_corpus_dirs = sorted(
        path.relative_to(root).as_posix()
        for path in (root / "tests" / "fuzzing" / "corpus").iterdir()
        if path.is_dir()
    )
    expected_corpus_dirs = sorted({target.corpus_dir for target in target_specs})
    if actual_corpus_dirs != expected_corpus_dirs:
        errors.append(
            "corpus directory set mismatch: "
            f"actual={actual_corpus_dirs} expected={expected_corpus_dirs}"
        )

    cargo_by_name = {item.name: item for item in cargo_bin_specs}
    cargo_names = sorted(cargo_by_name)
    cargo_paths = {item.path for item in cargo_bin_specs}
    if cargo_names != sorted(target_names):
        errors.append(
            "fuzz/Cargo.toml target set mismatch: "
            f"actual={cargo_names} expected={sorted(target_names)}"
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
        for seed_name in entry.seed_files:
            expected_payload = expected_payloads.get(seed_name)
            if expected_payload is None:
                errors.append(
                    f"{spec.name}: manifest seed {seed_name!r} has no deterministic generator"
                )
                continue
            seed_path = corpus_dir / seed_name
            if not seed_path.is_file():
                errors.append(f"{spec.name}: manifest seed {seed_name} missing on disk")
                continue
            actual_payload = seed_path.read_bytes()
            if not seed_payload_is_deterministic(
                seed_name, actual_payload, expected_payload
            ):
                errors.append(
                    f"{spec.name}: manifest seed {seed_name} is not deterministic "
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
    if manifest_targets != sorted(target_names):
        errors.append(
            "manifest target set mismatch: "
            f"actual={manifest_targets} expected={sorted(target_names)}"
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
    parser.add_argument("--targets-file", default=DEFAULT_TARGETS_FILE)
    args = parser.parse_args()

    root = pathlib.Path.cwd()

    if args.check:
        return check_seed_corpus(root, args.targets_file)

    targets_path = root / args.targets_file
    manifest_path = root / CORPUS_MANIFEST_PATH
    try:
        targets = load_target_specs(targets_path)
        manifest_entries = load_manifest_entries(manifest_path)
    except ValueError as error:
        raise SystemExit(str(error))
    if not targets:
        raise SystemExit(f"No targets found in {args.targets_file}")

    manifest_by_target: Dict[str, ManifestEntry] = {}
    for entry in manifest_entries:
        if entry.target in manifest_by_target:
            raise SystemExit(f"duplicate manifest entry for target {entry.target!r}")
        manifest_by_target[entry.target] = entry

    created = []
    for target in targets:
        manifest_entry = manifest_by_target.get(target.name)
        if manifest_entry is None:
            raise SystemExit(f"{target.name}: missing manifest entry")
        if manifest_entry.corpus_dir != target.corpus_dir:
            raise SystemExit(
                f"{target.name}: manifest corpus_dir {manifest_entry.corpus_dir!r} "
                f"does not match targets.toml {target.corpus_dir!r}"
            )
        created.extend(ensure_seed(root, target, manifest_entry.seed_files))

    if not args.ensure_only:
        manifest_path = write_manifest(root, manifest_entries)
        for item in created:
            print(item.as_posix())
        print(manifest_path.as_posix())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
