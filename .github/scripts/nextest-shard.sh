#!/usr/bin/env bash

# This file is part of midnight-zkir.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Run one shard of the test suite from a `cargo nextest archive`.
#
# A handful of proving tests take ~85% of the suite's runtime, so nextest's
# built-in --partition (slice/hash) leaves one shard with most of the work.
# Instead, the slow tests are assigned to shards by hand below, balanced by
# measured runtime, and the last shard runs everything else. That complement
# means every test still runs exactly once whatever happens to this list: a
# new slow test just lands in the last shard, and a renamed or removed one is
# caught by the check below rather than silently skipped.
#
# To rebalance: `cargo nextest run --release --profile ci` prints per-test
# times; keep each shard's total (incl. the last one) roughly equal.
#
# Usage: nextest-shard.sh --matrix
#          Print the shard numbers as a JSON array, for the CI job matrix (so
#          the matrix always matches the list below).
#        nextest-shard.sh <shard> <archive>
#          Run shard <shard> (1-based) from the given archive. Run from the
#          repo root, inside `nix develop .#test-runner`.
set -euo pipefail

# One entry per shard except the last; space-separated full test names.
# Runtimes on 18 cores (CI runners are slower, but in proportion).
SLOW=(
  # 21.0s + 4.1s
  "proof_tests::test_std_hashes_proof proof_tests::test_secp256k1_point_bytes_proof"
  # 15.2s + 7.4s + 3.0s
  "proof_tests::test_sha512_proof proof_tests::test_curve25519_point_bytes_proof proof_tests::test_ec_proof"
  # 14.3s + 8.9s
  "proof_tests::test_curve25519_ec_mul_proof proof_tests::test_secp256r1_point_bytes_proof"
  # last shard, everything else: ~23s, 9.2s of which is test_secp256r1_ec_mul_proof
)

COUNT=$(( ${#SLOW[@]} + 1 ))

if [ "${1:-}" = "--matrix" ]; then
  out=""
  for (( i = 1; i <= COUNT; i++ )); do out+="${out:+,}${i}"; done
  echo "[${out}]"
  exit 0
fi

SHARD="${1:?usage: nextest-shard.sh --matrix | <shard> <archive>}"
ARCHIVE="${2:?usage: nextest-shard.sh --matrix | <shard> <archive>}"

if [ "$SHARD" -lt 1 ] || [ "$SHARD" -gt "$COUNT" ]; then
  echo "::error::shard ${SHARD} out of range 1..${COUNT}"
  exit 1
fi

# filterset <names...>: matches exactly the named tests.
filterset() {
  local out="" t
  for t in "$@"; do out+="${out:+ | }test(=${t})"; done
  echo "$out"
}

# shellcheck disable=SC2206 # word splitting of the lists is intended
all_slow=(${SLOW[*]})
nextest=(cargo-nextest nextest)
# The JUnit report goes to target/nextest/ci/ as usual (via the remap). The
# archive is extracted under target/ because temp dirs may be noexec.
from_archive=(--archive-file "$ARCHIVE" --workspace-remap . --extract-to target/nextest-archive --extract-overwrite)
mkdir -p target/nextest-archive

# Every named test must exist, or it would silently stop running anywhere.
# --color never: CI sets CARGO_TERM_COLOR=always, which nextest honours even
# when piped, and the escape codes would stop the names from matching.
listed="$("${nextest[@]}" list "${from_archive[@]}" -E "$(filterset "${all_slow[@]}")" \
  --color never --message-format oneline | awk '{print $2}' | sort -u)"
missing="$(comm -23 <(printf '%s\n' "${all_slow[@]}" | sort -u) <(printf '%s\n' "$listed"))"
if [ -n "$missing" ]; then
  echo "::error::Tests named in .github/scripts/nextest-shard.sh no longer exist: $(echo "$missing" | tr '\n' ' ')- update the shard lists."
  exit 1
fi

if [ "$SHARD" -lt "$COUNT" ]; then
  # shellcheck disable=SC2206
  mine=(${SLOW[$(( SHARD - 1 ))]})
  filter="$(filterset "${mine[@]}")"
else
  filter="not ($(filterset "${all_slow[@]}"))"
fi

echo "Shard ${SHARD}/${COUNT}: ${filter}"
"${nextest[@]}" run "${from_archive[@]}" --profile ci -E "$filter"
