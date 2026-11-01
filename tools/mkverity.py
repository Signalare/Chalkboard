#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Chalkboard dm-verity tool.

Computes standard Linux dm-verity hash trees (SHA-256, 4096-byte blocks)
for squashfs root filesystems, outputs the dm-mod.create command line parameters,
and verifies existing hash trees.
"""

import argparse
import hashlib
import math
import os
import sys

BLOCK_SIZE = 4096
HASH_SIZE = 32  # SHA-256
HASHES_PER_BLOCK = BLOCK_SIZE // HASH_SIZE  # 128


def compute_verity_tree(data_bytes: bytes, salt_bytes: bytes):
    """
    Computes dm-verity Merkle tree over data_bytes with salt_bytes.
    Returns (hash_tree_bytes, root_hash_hex, num_data_blocks, hash_start_block).
    """
    total_data_len = len(data_bytes)
    num_data_blocks = math.ceil(total_data_len / BLOCK_SIZE)
    if num_data_blocks == 0:
        raise ValueError("Data cannot be empty")

    # Pad data to multiple of BLOCK_SIZE if needed
    padded_data_len = num_data_blocks * BLOCK_SIZE
    if len(data_bytes) < padded_data_len:
        data_bytes = data_bytes + b"\x00" * (padded_data_len - len(data_bytes))

    # Calculate levels
    # Level 0 hashes the data blocks
    # Level 1 hashes Level 0 blocks, etc.
    levels = []
    current_blocks = num_data_blocks
    while current_blocks > 1:
        needed_hash_blocks = math.ceil(current_blocks / HASHES_PER_BLOCK)
        levels.append((current_blocks, needed_hash_blocks))
        current_blocks = needed_hash_blocks

    if not levels:
        # Only 1 data block: root hash is hash of that single block
        block = data_bytes[0:BLOCK_SIZE]
        root_digest = hashlib.sha256(salt_bytes + block).hexdigest()
        return b"", root_digest, num_data_blocks, num_data_blocks

    # Now compute level hashes
    # dm-verity on-disk order:
    # Linux kernel dm-verity stores levels from root-downwards or bottom-up depending on format:
    # In standard dm-verity hash tree format, levels are stored from top level down to level 0:
    # level N-1, level N-2, ..., level 1, level 0.
    # Let's compute from level 0 upwards, then reverse when assembling.
    computed_levels = []

    # Level 0:
    l0_hashes = bytearray()
    for i in range(num_data_blocks):
        block = data_bytes[i * BLOCK_SIZE : (i + 1) * BLOCK_SIZE]
        h = hashlib.sha256(salt_bytes + block).digest()
        l0_hashes.extend(h)

    # Pad level 0 to full blocks
    pad_len = math.ceil(len(l0_hashes) / BLOCK_SIZE) * BLOCK_SIZE - len(l0_hashes)
    l0_hashes.extend(b"\x00" * pad_len)
    computed_levels.append(bytes(l0_hashes))

    # Higher levels:
    for lvl_idx in range(len(levels) - 1):
        prev_level_bytes = computed_levels[-1]
        prev_num_blocks = len(prev_level_bytes) // BLOCK_SIZE
        cur_hashes = bytearray()
        for b_idx in range(prev_num_blocks):
            block = prev_level_bytes[b_idx * BLOCK_SIZE : (b_idx + 1) * BLOCK_SIZE]
            h = hashlib.sha256(salt_bytes + block).digest()
            cur_hashes.extend(h)
        pad_len = math.ceil(len(cur_hashes) / BLOCK_SIZE) * BLOCK_SIZE - len(cur_hashes)
        cur_hashes.extend(b"\x00" * pad_len)
        computed_levels.append(bytes(cur_hashes))

    # Top level:
    top_level_bytes = computed_levels[-1]
    # The top level has exactly 1 block (or fewer hashes in 1 block)
    top_block = top_level_bytes[0:BLOCK_SIZE]
    root_digest = hashlib.sha256(salt_bytes + top_block).hexdigest()

    # Assemble hash tree: levels are ordered top-to-bottom (highest level first, level 0 last)
    # per kernel verity format
    tree_bytes = bytearray()
    for lvl in reversed(computed_levels):
        tree_bytes.extend(lvl)

    hash_start_block = num_data_blocks
    return bytes(tree_bytes), root_digest, num_data_blocks, hash_start_block


def main():
    parser = argparse.ArgumentParser(description="Chalkboard dm-verity tool")
    parser.add_argument("image", help="Path to squashfs image")
    parser.add_argument("--append", action="store_true", help="Append hash tree to image file")
    parser.add_argument("--output", help="Output combined or hash-tree file")
    parser.add_argument("--salt", default="5c68616c6b626f6172645f73616c745f323032365f7369676e616c6172652121", help="Salt in hex (default 32 bytes)")
    parser.add_argument("--device", default="/dev/mmcblk0p5", help="Target block device for table line")
    parser.add_argument("--verify", action="store_true", help="Verify image and check root hash")
    parser.add_argument("--expected-root", help="Expected root hash for verification")

    args = parser.parse_args()

    salt_bytes = bytes.fromhex(args.salt)

    with open(args.image, "rb") as f:
        data = f.read()

    tree_bytes, root_hash, num_blocks, hash_start = compute_verity_tree(data, salt_bytes)
    sectors = num_blocks * (BLOCK_SIZE // 512)

    table_line = (
        f"0 {sectors} verity 1 {args.device} {args.device} "
        f"{BLOCK_SIZE} {BLOCK_SIZE} {num_blocks} {hash_start} sha256 "
        f"{root_hash} {args.salt} 1 restart_on_corruption"
    )

    dm_mod_create = f'rootfs,,ro,{table_line}'

    if args.verify:
        if args.expected_root:
            if root_hash.lower() == args.expected_root.lower():
                print(f"VERITY OK: root hash matches {root_hash}")
                sys.exit(0)
            else:
                print(f"VERITY MISMATCH: calculated {root_hash} != expected {args.expected_root}", file=sys.stderr)
                sys.exit(1)
        else:
            print(f"Calculated root hash: {root_hash}")
            sys.exit(0)

    if args.append:
        with open(args.image, "ab") as f:
            f.write(tree_bytes)
        print(f"Appended {len(tree_bytes)} bytes of hash tree to {args.image}")

    if args.output:
        with open(args.output, "wb") as f:
            if args.output != args.image:
                f.write(data)
            f.write(tree_bytes)
        print(f"Wrote image + hash tree to {args.output}")

    print(f"DATA_BLOCKS={num_blocks}")
    print(f"DATA_SECTORS={sectors}")
    print(f"HASH_START_BLOCK={hash_start}")
    print(f"ROOT_HASH={root_hash}")
    print(f"SALT={args.salt}")
    print(f"TABLE_LINE={table_line}")
    print(f"DM_MOD_CREATE={dm_mod_create}")


if __name__ == "__main__":
    main()
