# Ledger9 V3 proving context

The ZKIR 3.2.1 source-line proposal forwards an explicit, request-owned `ProverContext` through V3 key loading and proving. Its default remains the released heap path. A native caller may choose mapped-key and coset policy when the pinned `midnight-proofs` build has `native-memory`; a Wasm caller requesting native storage receives an unsupported error.

The V3 key initializer runs once and preserves the original load error. The context's cancellation token is checked while waiting for key admission and during proving phase transitions. It belongs to one request; cancelling it does not cancel a separate proof. This extends the V3 path without a second key representation or changes to serialized IR, key, or proof tags.

The published 3.2.1 crate records source commit `8825fb5a`. Its Git release tag points to a commit whose manifest still declares 3.2.0, so compatibility comparisons use the published source revision. Ledger9 consumes a separately pinned integration revision of this source proposal. See [Ledger #26](https://github.com/betsudo/midnight-ledger/issues/26) for TypeScript/Rust parity and [ZK #5](https://github.com/betsudo/midnight-zk/issues/5) for released-artifact qualification.
