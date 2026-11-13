# Ledger9 ZKIR v3 browser-target memory gate

The local ZKIR v3 memory-policy adapter at `0e03d5e` was checked against
Ledger `f74c4f72` and ZK `de847239` on 2026-11-13. This follows the
native Ledger9 HTTP and generated-key tests recorded in the Ledger branch's
`docs/ledger9-local-sync.md`.

`cargo check --offline --target wasm32-unknown-unknown -p midnight-zkir-wasm`
passed with Homebrew LLVM's `clang` selected through
`CC_wasm32_unknown_unknown`. The command used temporary Cargo path patches
for Ledger's base-crypto, serialize, and transient-crypto, and ZK's curves,
circuits, proofs, and zk-stdlib. It also patched the upstream ZK Git source
used by `sha3-circuit` to the same local proofs tree. The target dependency
tree resolved one `midnight-proofs` 0.8.3 source. Its enabled features were
`circuit-params`, `committed-instances`, and `default`; the Wasm target
tree had no `memmap2` dependency or `disk-spill` feature.

This is a compile and dependency-boundary check. It does not exercise browser
runtime behavior or a published package graph. The Cargo lockfile update
required for the temporary local patches was restored after the check.

## Distinct local package versions

On 2026-11-13 the same Wasm check passed after selecting local ZK curves
0.3.2, circuits 7.3.1, proofs 0.8.4, and stdlib 2.5.0, with Ledger
transient-crypto 3.2.0 and this ZKIR 3.3.0 source. Cargo's Wasm dependency
tree resolved one proofs 0.8.4 and no `memmap2`, `mmap`, or `disk-spill`
activation. Temporary path patches supplied the unpublished source stack;
the result remains a compile gate, not browser runtime or release evidence.
