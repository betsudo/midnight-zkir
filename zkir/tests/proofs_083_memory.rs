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
use transient_crypto::{
    curve::Fr,
    hash::transient_commit,
    proofs::{
        CompanionCache, KeyLocation, MappedParams, ParamsProver, ParamsProverProvider,
        ParamsVerifier, ProofPreimage, ProverKey, ProvingKeyMaterial, Resolver, TrustedParamsDir,
        VerifierKey, Zkir,
    },
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
    exercise_proof_paths(ir, vec![1.into()], None, vec![], (4, 3)).await;
}

#[actix_rt::test]
async fn compact_generated_ledger9_counter_proves_with_mapped_spill() {
    // Compact 0.36.1 (7fa3a117c) emitted this v3.1 IR from the adjacent
    // counter.compact source. It exercises Ledger9 impact and communications
    // commitment, which the minimal test does not.
    let ir = IrSource::load(include_str!("compact-v3-counter/increment.zkir").as_bytes())
        .expect("load Compact-generated v3 IR");
    let opening: Fr = 7.into();
    let commitment = transient_commit(&[] as &[Fr], opening);
    let public_transcript_inputs = [0x70, 0x01, 0x01, 0x00, 0x0e, 0x01, 0xa1]
        .into_iter()
        .map(Fr::from)
        .collect();
    exercise_proof_paths(
        ir,
        vec![],
        Some((commitment, opening)),
        public_transcript_inputs,
        (6, 31),
    )
    .await;
}

#[test]
fn compact_generated_hash_chain_cost_model_matches_ledger9() {
    // Ledger9's 0.8.2 baseline for this Compact 0.36.1 circuit is k=15 and
    // 32,185 rows. Detect complexity drift before running larger proofs.
    let ir = IrSource::load(include_str!("compact-v3-hash-chain/hashAndIncrement.zkir").as_bytes())
        .expect("load Compact-generated hash-chain IR");
    assert_eq!((ir.k(), ir.model().rows()), (15, 32_185));
}

async fn exercise_proof_paths(
    ir: IrSource,
    inputs: Vec<Fr>,
    communications_commitment: Option<(Fr, Fr)>,
    public_transcript_inputs: Vec<Fr>,
    expected_k_rows: (u8, usize),
) {
    let k = ir.k();
    let rows = ir.model().rows();
    assert_eq!((k, rows), expected_k_rows);
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
        communications_commitment,
        inputs,
        private_transcript: vec![],
        public_transcript_inputs,
        public_transcript_outputs: vec![],
        key_location: KeyLocation(Cow::Borrowed("test")),
    };
    let mut public_inputs = vec![Fr::from(42)];
    if let Some((commitment, _)) = communications_commitment {
        public_inputs.push(commitment);
    }
    public_inputs.extend(preimage.public_transcript_inputs.iter().copied());
    let rng = || ChaCha20Rng::from_seed([42; 32]);
    let (heap_proof, _) = preimage
        .prove::<IrSource>(rng(), &heap, &resolver)
        .await
        .expect("heap proof");
    resolver
        .vk
        .verify(&verifier_params, &heap_proof, public_inputs.iter().copied())
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
        .verify(
            &verifier_params,
            &mapped_proof,
            public_inputs.iter().copied(),
        )
        .expect("verify mapped proof");
    assert_eq!(heap_proof.0.len(), mapped_proof.0.len());
    // Proof bytes are not deterministic: proofs 0.8.3 draws quotient-limb
    // blinding from OsRng even when the caller supplies a seeded RNG.
    public_inputs[0] = 43.into();
    assert!(
        resolver
            .vk
            .verify(&verifier_params, &mapped_proof, public_inputs.into_iter())
            .is_err()
    );
}
