# Ledger9 V3 proving context

The ZKIR 3.2.1 source-line proposal forwards an explicit, request-owned `ProverContext` through V3 key loading and proving. Its default remains the released heap path. A native caller may choose mapped-key and coset policy when the pinned `midnight-proofs` build has `native-memory`; a Wasm caller requesting native storage receives an unsupported error.

## Boundary and call flow

```text
Ledger V3 ProofPreimage
  -> transient-crypto Zkir::prove_with_context
  -> zkir IrSource::prove_with_context
  -> ProverKey::init_with_context (once)
  -> parameters + preimage preprocessing
  -> midnight-zk-stdlib::prove_with_context
  -> unchanged V3 proof, public inputs, and skip list
```

The implementation in [`zkir/src/ir.rs`](../zkir/src/ir.rs) uses one
`prove_impl` for the released `prove` path and the new context-aware
path. The former calls `pk.init()` and `midnight_zk_stdlib::prove`; the
latter passes the same context to `pk.init_with_context()` and
`midnight_zk_stdlib::prove_with_context`. The key initialization error is
returned directly before requesting parameters or preprocessing. This
keeps the policy at the request boundary instead of adding process-global
state to ZKIR.

The V3 key initializer runs once and preserves the original load error. The context's cancellation token is checked while waiting for key admission and during proving phase transitions. It belongs to one request; cancelling it does not cancel a separate proof. This extends the V3 path without a second key representation or changes to serialized IR, key, or proof tags.

A freshly generated key and a key already initialized through the legacy API can be reused when key storage stays on the heap. This also permits coset-only spill after the native policy is validated. A mapped-key request must receive serialized key bytes so the requested backing policy can be applied; it rejects an already initialized key instead of silently retaining its prior storage. The minimal V3 proof test covers both initialized-key cases with the official k=4 parameters; Ledger's release-source fixture test covers initialized-key coset-only spill.

| Key state at entry | Heap key policy | Mapped-key policy |
| --- | --- | --- |
| Serialized bytes | Initialize or use the matching policy cache | Initialize with mapped backing |
| Generated or already initialized | Reuse; coset-only spill remains possible | Reject explicitly: serialized bytes required |

The context's `CancelToken` applies to key-build admission, cache lookup,
and later prover phase checkpoints. It is cooperative: a single low-level
key read and transcript work already completed cannot be rolled back.
Callers must discard the transcript after an error. Two requests only share
cancellation when they deliberately share a token.

## Compatibility contract

The serialized `ir-source[v3-generic]` and `ir-minor-version[v3]` tags,
the proving-key representation, and proof format stay unchanged. This
branch adds an API route and storage policy, not a new IR variant. Legacy
callers continue through `Zkir::prove`. Native storage is opt-in both at
build time (`native-memory`) and per request; Wasm requests for mapped
storage fail with `Unsupported`, while heap proving remains available.

The published 3.2.1 crate records source commit `8825fb5a`. Its Git release tag points to a commit whose manifest still declares 3.2.0, so compatibility comparisons use the published source revision. Ledger9 consumes a separately pinned integration revision of this source proposal. See [Ledger #26](https://github.com/betsudo/midnight-ledger/issues/26) for TypeScript/Rust parity and [ZK #5](https://github.com/betsudo/midnight-zk/issues/5) for released-artifact qualification.

## Upstream review and qualification

The dependency graph is pinned in [`Cargo.toml`](../Cargo.toml): published
ZKIR 3.2.1 source plus the Ledger V3 context bridge and proofs storage API.
Review the source alignment and the small `ir.rs` API change separately.
Before promotion, confirm native default and feature builds, Wasm heap and
feature builds, generated/initialized/serialized key cases, cancellation,
and a V3 TypeScript preimage/Node-Wasm-to-native proof exchange. The
small-circuit exchange has passed locally. Keep exact dependency heads and
test artifacts with that evidence.

## Ledger9 artifact qualification

[Ledger PR #39](https://github.com/betsudo/midnight-ledger/pull/39)
uses the official k=4 SRS with a key generated locally from published
ZKIR 3.2.1 source. It separately uses the official k=6 SRS and an
independently published Midnight JS Compact 0.36.1 testkit V3 key; the
key, verifier, and IR hashes match the pinned SDK manifest. Optimized
native Ledger9 proves and verifies with heap, mapped SRS/key, and forced
coset spill. An opt-in local Node/Wasm test proves with the published
testkit key and TypeScript preimage, then verifies in native Ledger9.
Ordinary hosted CI does not run that cross-runtime test.

At executable Ledger head `2b3ee462`, the focused native-memory V3
artifact test, feature-enabled Wasm target compilation, eight-job
[build-and-test workflow](https://github.com/betsudo/midnight-ledger/actions/runs/43834105721),
and [coverage](https://github.com/betsudo/midnight-ledger/actions/runs/43834104987)
passed. These k=4/k=6 cases establish format and interoperability; they
do not establish high-`k` memory benefit, browser execution, or
deployed-dApp key coverage. Those gates remain under
[Ledger #30](https://github.com/betsudo/midnight-ledger/issues/30),
[Ledger #12](https://github.com/betsudo/midnight-ledger/issues/12), and
[Ledger #26](https://github.com/betsudo/midnight-ledger/issues/26).
The cross-repository optimization PR map and upstream landing order are
in [ZK PR #13](https://github.com/betsudo/midnight-zk/pull/13).
