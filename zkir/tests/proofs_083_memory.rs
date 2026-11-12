// This file is part of midnight-ledger.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0

#![cfg(not(target_family = "wasm"))]

//! Exercise ZKIR v3 through the proofs 0.8.3 memory paths with a small,
//! generated SRS. No external parameter file or proof server is required.

use std::{borrow::Cow, io, sync::Arc};

use base_crypto::data_provider::MidnightDataProvider;
use midnight_curves::Bls12;
use midnight_proofs::{
    config::{CancelToken, ProverConfig, ProverContext},
    poly::kzg::params::ParamsKZG,
    utils::SerdeFormat,
};
use midnight_zkir::IrSource;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serialize::tagged_serialize;
use transient_crypto::proofs::{
    CompanionCache, KeyLocation, MappedParams, ParamsProver, ParamsProverProvider, ParamsVerifier,
    ProofPreimage, ProverKey, ProvingKeyMaterial, Resolver, TrustedParamsDir, VerifierKey, Zkir,
};

struct HeapParams(ParamsProver);

impl ParamsProverProvider for HeapParams {
    async fn get_params(&self, _k: u8) -> io::Result<ParamsProver> {
        Ok(self.0.clone())
    }
}

struct NoFallback;

impl ParamsProverProvider for NoFallback {
    async fn get_params(&self, _k: u8) -> io::Result<ParamsProver> {
        panic!("mapped companion was not selected")
    }
}

struct KeyResolver {
    pk: ProverKey<IrSource>,
    vk: VerifierKey,
    ir: IrSource,
}

impl Resolver for KeyResolver {
    async fn resolve_key(&self, _key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        let mut prover_key = Vec::new();
        let mut verifier_key = Vec::new();
        let mut ir_source = Vec::new();
        tagged_serialize(&self.pk, &mut prover_key)?;
        tagged_serialize(&self.vk, &mut verifier_key)?;
        tagged_serialize(&self.ir, &mut ir_source)?;
        Ok(Some(ProvingKeyMaterial {
            prover_key,
            verifier_key,
            ir_source,
        }))
    }
}

#[actix_rt::test]
async fn generated_srs_proves_with_heap_and_mapped_spill_paths() {
    let ir = IrSource::load(
        &br#"{
            "version": { "major": 3, "minor": 0 },
            "inputs": [{ "name": "%v_0", "type": "Scalar<BLS12-381>" }],
            "outputs": [],
            "do_communications_commitment": false,
            "instructions": [{ "op": "assert", "cond": "%v_0" }]
        }"#[..],
    )
    .expect("load v3 IR");
    let k = ir.k();
    let params = ParamsProver(Arc::new(ParamsKZG::<Bls12>::unsafe_setup(
        k as u32,
        ChaCha20Rng::from_seed([7; 32]),
    )));
    let mut params_bytes = Vec::new();
    params
        .0
        .write_custom(&mut params_bytes, SerdeFormat::RawBytesUnchecked)
        .expect("serialize verifier parameters");
    let verifier_params =
        ParamsVerifier::read(&params_bytes[..]).expect("read verifier parameters");

    let heap = HeapParams(params.clone());
    let (pk, vk) = ir.keygen(&heap).await.expect("v3 keygen");
    let resolver = KeyResolver {
        pk,
        vk,
        ir: ir.clone(),
    };
    let preimage = ProofPreimage {
        binding_input: 42.into(),
        communications_commitment: None,
        inputs: vec![1.into()],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        key_location: KeyLocation(Cow::Borrowed("test")),
    };
    let rng = || ChaCha20Rng::from_seed([42; 32]);
    let (heap_proof, _) = preimage
        .prove::<IrSource>(rng(), &heap, &resolver)
        .await
        .expect("heap proof");
    resolver
        .vk
        .verify(&verifier_params, &heap_proof, [42.into()].into_iter())
        .expect("verify heap proof");

    let params_dir = tempfile::tempdir().expect("private companion directory");
    // This test publishes the companion through the cache and keeps the
    // directory immutable until every mapped view is dropped.
    let trusted = unsafe { TrustedParamsDir::new(params_dir.path()) };
    let cache = CompanionCache::from_trusted_dir(trusted.clone());
    cache
        .open_or_build(&MidnightDataProvider::name_k(k), || Ok(params))
        .expect("publish companion");
    let mapped = MappedParams::new(NoFallback, trusted);
    let spill_dir = tempfile::tempdir().expect("private spill directory");
    let mut policy = ProverConfig::heap();
    policy.spill_cosets = true;
    policy.coset_spill_floor_k = 0;
    policy.spill_dir = Some(spill_dir.path().to_path_buf());
    assert!(policy.spills_cosets_at(k as u32));
    let context = ProverContext::new(policy);
    let token = CancelToken::new();
    token.cancel();
    let cancelled = context.clone().with_cancel(token);
    let error = preimage
        .prove_with_context::<IrSource>(rng(), &mapped, &resolver, &cancelled)
        .await
        .expect_err("cancelled request must stop");
    assert!(matches!(
        error.downcast_ref::<midnight_proofs::plonk::Error>(),
        Some(midnight_proofs::plonk::Error::Cancelled { .. })
    ));

    let (mapped_proof, _) = preimage
        .prove_with_context::<IrSource>(rng(), &mapped, &resolver, &context)
        .await
        .expect("mapped and spilled proof");
    resolver
        .vk
        .verify(&verifier_params, &mapped_proof, [42.into()].into_iter())
        .expect("verify mapped proof");
    // Proof bytes are not deterministic: proofs 0.8.3 draws quotient-limb
    // blinding from OsRng even when the caller supplies a seeded RNG.
    assert!(
        resolver
            .vk
            .verify(&verifier_params, &mapped_proof, [43.into()].into_iter())
            .is_err()
    );
}
