// This file is part of midnight-ledger.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `inner_proof` consumes one entry of `ProofPreimage::inner_proofs` per
//! guarded-on instruction. Its pairing with `prepare_verification` is checked
//! before either pass runs.
//!
//! Every guard here is a constant `0x00`, which is what keeps these tests fast:
//! a guarded-off `prepare_verification` returns the trivial accumulator without reading
//! its verifying key, so the side-table entry can be a stub. Guarded-on
//! verification needs a real key and a real inner proof, and is covered by
//! `verify_proof_e2e`.

use std::borrow::Cow;

use sha2::Digest;

use midnight_zkir::IrSource;
use transient_crypto::curve::Fr;
use transient_crypto::proofs::{InnerProofWitness, KeyLocation, ProofPreimage, Zkir};

/// Stands in for a verifying key. Never parsed, because nothing here is
/// verified; `inner_vks` is only indexed by digest.
const STUB_VK: &[u8] = &[0u8, 1, 2, 3];

fn preimage(inner_proofs: Vec<InnerProofWitness>) -> ProofPreimage {
    ProofPreimage {
        binding_input: Fr::from(7u64),
        communications_commitment: None,
        inputs: vec![],
        private_transcript: vec![],
        public_transcript_inputs: vec![],
        public_transcript_outputs: vec![],
        inner_proofs,
        key_location: KeyLocation(Cow::Borrowed("builtin")),
    }
}

fn blank() -> InnerProofWitness {
    InnerProofWitness::Direct(vec![])
}

/// The rejection message, so each test can say *why* it expected a rejection
/// rather than accept any error at all.
fn rejection(instructions: Vec<String>, witnesses: usize) -> String {
    load(instructions)
        .check(&preimage(vec![blank(); witnesses]))
        .expect_err("must be rejected")
        .to_string()
}

fn inner(name: &str, guard: &str) -> String {
    format!(r#"{{ "op": "inner_proof", "guard": "{guard}", "output": "{name}" }}"#)
}

fn verify(name: &str, guard: &str) -> String {
    format!(
        r#"{{ "op": "prepare_verification", "guard": "{guard}", "vk_hash": "0x{vk}",
              "instance": [], "proof": "{name}", "output": "{name}_acc" }},
           {{ "op": "verify_accumulator", "input": "{name}_acc" }}"#,
        vk = const_hex::encode(sha2::Sha256::digest(STUB_VK)),
    )
}

/// A well-formed, guarded-off `inner_proof` / `prepare_verification` pair.
fn pair(name: &str) -> Vec<String> {
    vec![inner(name, "0x00"), verify(name, "0x00")]
}

fn load(instructions: Vec<String>) -> IrSource {
    let ir_json = format!(
        r#"{{
           "version": {{ "major": 3, "minor": 3 }},
           "inputs": [],
           "outputs": [],
           "do_communications_commitment": false,
           "instructions": [{}]
        }}"#,
        instructions.join(",\n")
    );
    let mut ir = IrSource::load(ir_json.as_bytes()).expect("IR must parse");
    ir.inner_vks = vec![STUB_VK.to_vec()];
    ir
}

#[test]
fn a_guarded_off_instruction_consumes_no_proof_witness() {
    let ir = load([pair("%p_0"), pair("%p_1")].concat());
    ir.check(&preimage(vec![]))
        .expect("both guarded off, no witnesses");

    let msg = rejection([pair("%p_0"), pair("%p_1")].concat(), 1);
    assert!(
        msg.contains("Expected 0 proof witnesses (one per guarded-on InnerProof), received 1"),
        "a surplus must be refused on the count, got: {msg}"
    );
}

#[test]
fn a_verify_proof_must_name_a_proof_bound_before_it() {
    let msg = rejection(vec![verify("%p_9", "0x00")], 0);
    assert!(msg.contains("no preceding `inner_proof` binds"), "{msg}");

    // Binding it later does not help: both passes resolve in instruction order.
    let msg = rejection(vec![verify("%p_0", "0x00"), inner("%p_0", "0x00")], 1);
    assert!(msg.contains("no preceding `inner_proof` binds"), "{msg}");
}

#[test]
fn a_proof_name_is_bound_only_once() {
    // A second binding would shadow the first.
    let rebound = vec![
        inner("%p_0", "0x00"),
        inner("%p_0", "0x00"),
        verify("%p_0", "0x00"),
    ];
    let msg = rejection(rebound, 2);
    assert!(msg.contains("`inner_proof` rebinds %p_0"), "{msg}");
}

#[test]
fn the_two_guards_must_agree() {
    let msg = rejection(vec![inner("%p_0", "0x01"), verify("%p_0", "0x00")], 1);
    assert!(msg.contains("guarded differently"), "{msg}");
}

#[test]
fn a_bound_proof_is_verified_exactly_once() {
    let twice = vec![
        inner("%p_0", "0x00"),
        verify("%p_0", "0x00"),
        verify("%p_0", "0x00"),
    ];
    let msg = rejection(twice, 1);
    assert!(msg.contains("verified more than once"), "{msg}");

    let msg = rejection(vec![inner("%p_0", "0x00")], 1);
    assert!(msg.contains("no `prepare_verification` uses"), "{msg}");
}
