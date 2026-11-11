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

//! The `prepare_verification` instruction: partially verify an inner Midnight proof,
//! producing the (deferred) accumulator it leaves for the final pairing. Built
//! directly on the verifier-gadget primitives that midnight-circuits exposes.
//! The accumulator is only checked once a `verify_accumulator` exposes it; the
//! verifier side (reconstructing each accumulator from the public inputs and
//! running its pairing check) lives in `transient-crypto`. [`verify_inner_proof`]
//! runs that same pairing check here too, against the embedded Midnight SRS
//! point, for a standalone verdict.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use anyhow::{anyhow, bail};
use group::Group;
use midnight_circuits::hash::poseidon::PoseidonState;
use midnight_circuits::instructions::AssignmentInstructions;
use midnight_circuits::types::{AssignedBit, AssignedNative};
use midnight_circuits::verifier::{Accumulator, AssignedAccumulator, SelfEmulation, fixed_bases};
use midnight_curves::Bls12;
use midnight_proofs::{
    circuit::{Layouter, Value},
    plonk::{self, Error},
    poly::kzg::{KZGCommitmentScheme, params::ParamsVerifierKZG},
    transcript::{CircuitTranscript, Transcript},
    utils::SerdeFormat,
};
use midnight_zk_stdlib::{MidnightVK, ZkStdLib};
use sha2::{Digest, Sha256};
use transient_crypto::curve::outer;
use transient_crypto::proofs::{DeferredAccumulator, InnerSelfEmulation as S};

use crate::ir::{Identifier, Instruction as I, IrSource, Operand};
use crate::ir_instructions::accumulate::trivial_accumulator;

/// Label prefix for an inner verifying key's fixed bases.
///
/// Derived from the blob's own digest.
fn vk_name(vk_blob: &[u8]) -> String {
    format!("inner_vk_{}", const_hex::encode(Sha256::digest(vk_blob)))
}

/// Off-circuit half of `prepare_verification`: partially verifies the inner proof and
/// returns the accumulator the check defers, resolved and collapsed.
///
/// If `guard` is `false` neither key nor proof is read and the trivial
/// accumulator is returned.
pub fn prepare_verification_offcircuit(
    vk_blob: &[u8],
    instance: &[outer::Scalar],
    proof: &[u8],
    guard: bool,
) -> anyhow::Result<DeferredAccumulator> {
    if !guard {
        return Ok(trivial_accumulator());
    }

    let vk = MidnightVK::read(&mut &vk_blob[..], SerdeFormat::Processed)
        .map_err(|e| anyhow!("reading inner verifying key: {e}"))?;
    let plonk_vk = vk.vk();
    let vk_name = vk_name(vk_blob);
    let bases = fixed_bases::<S>(&vk_name, plonk_vk);

    let mut transcript = CircuitTranscript::<PoseidonState<outer::Scalar>>::init_from_bytes(proof);
    let dual_msm = plonk::prepare::<
        outer::Scalar,
        KZGCommitmentScheme<Bls12>,
        CircuitTranscript<PoseidonState<outer::Scalar>>,
    >(
        plonk_vk,
        // A single identity committed instance: committed instances are not
        // supported yet.
        &[&[<S as SelfEmulation>::C::identity()]],
        &[&[instance]],
        &mut transcript,
    )?;

    let mut acc = Accumulator::<S>::from_dual_msm(dual_msm, &vk_name, &bases);
    acc.resolve_fixed_bases(&bases);
    acc.collapse();

    DeferredAccumulator::new(&acc)
        .ok_or_else(|| anyhow!("the inner proof's accumulator failed to collapse"))
}

/// In-circuit half of [`prepare_verification_offcircuit`]: verifies the inner proof and
/// returns its accumulator, resolved and collapsed.
///
/// If `guard` is `0` the accumulator is reduced to the trivial one, so the
/// deferred pairing holds whatever the prover supplied.
pub fn prepare_verification_incircuit(
    std: &ZkStdLib,
    layouter: &mut impl Layouter<outer::Scalar>,
    vk_blob: &[u8],
    instance: &[AssignedNative<outer::Scalar>],
    proof: Value<Vec<u8>>,
    guard: &AssignedBit<outer::Scalar>,
) -> Result<AssignedAccumulator<S>, Error> {
    let vk = MidnightVK::read(&mut &vk_blob[..], SerdeFormat::Processed)
        .map_err(|e| Error::Synthesis(format!("inner verifying key: {e}")))?;
    let plonk_vk = vk.vk();
    let vk_name = vk_name(vk_blob);
    let verifier = std.verifier();
    let bls = std.bls12_381();
    let scalar_chip = bls.scalar_field_chip();

    // Only `transcript_repr` enters the transcript, but it cannot disagree
    // with the key: `MidnightVK::read` never reads it, `VerifyingKey::from_parts`
    // recomputes it from the commitments, domain and cs parsed here, the same
    // ones the fixed bases below are taken from.
    let assigned_vk = verifier.assign_fixed_vk(
        layouter,
        &vk_name,
        plonk_vk.get_domain(),
        plonk_vk.cs(),
        plonk_vk.transcript_repr(),
    )?;

    // Assign the inner VK's fixed bases in-circuit, keyed by the same names
    // `fixed_bases` produces.
    let mut assigned_bases = BTreeMap::new();
    for (name, base) in fixed_bases::<S>(&vk_name, plonk_vk) {
        assigned_bases.insert(name, bls.assign_fixed(layouter, base)?);
    }

    // The committed instance is a single identity point (we do not support
    // committed instances), mirroring the off-circuit `&[&[C::identity()]]`.
    let committed = [bls.assign_fixed(layouter, <S as SelfEmulation>::C::identity())?];

    let mut acc = verifier.prepare(layouter, &assigned_vk, &committed, &[instance], proof)?;
    acc.resolve_fixed_bases(&assigned_bases);
    AssignedAccumulator::scale_by_bit(layouter, scalar_chip, guard, &mut acc)?;
    acc.collapse(layouter, bls, scalar_chip)?;
    Ok(acc)
}

/// `s_g2` of the Midnight SRS.
///
/// Only used off-circuit, by [`verify_inner_proof`]. Soundness does not rely on
/// this point being valid with respect to the original SRS.
const SRS_S_G2: &str = concat!(
    // x1, x0, y1, y0, as the raw encoding orders them.
    "04bbe1a24fcc4f988c6ef268d0c1160eac0a0c4f53d80bd74f3d2e4667be27408625a83825354e27c70859883102eb43",
    "07acb569b3187c0fd1993980aa52a6e9e2080b9697fab96abd5c5f1c3b988256f2d99366f1bbccf13cf0e20702fee18c",
    "0f08179604abe8c1820bbed11127408dcb5072adcab85774a32d518b682350b06a22a87bf17887732a0af45ddfccbff8",
    "10e1b221e2a5911d6f9683a36f4e7fe7d44281de86a186cc106c3336f58fece01e766dc4147d38e31936105a4944cb59",
);

static SRS_VERIFIER: LazyLock<ParamsVerifierKZG<Bls12>> = LazyLock::new(|| {
    let s_g2 = const_hex::decode(SRS_S_G2).expect("`SRS_S_G2` is hex");
    ParamsVerifierKZG::read(&mut &s_g2[..], SerdeFormat::RawBytesUnchecked)
        .expect("`SRS_S_G2` is a raw-encoded G2 point")
});

/// Verifies an inner proof off-circuit, pairing included.
///
/// Only the proof's own accumulator is checked; one carried in its instance is
/// left to the outer circuit.
pub fn verify_inner_proof(
    vk_blob: &[u8],
    instance: &[outer::Scalar],
    proof: &[u8],
) -> anyhow::Result<()> {
    // Guard `true`: there is no instruction here to guard off, only a caller
    // asking whether this proof holds.
    let acc = prepare_verification_offcircuit(vk_blob, instance, proof, true)?;

    if !acc.to_accumulator().check(&SRS_VERIFIER, &BTreeMap::new()) {
        bail!("the inner proof does not hold for this verifying key and instance");
    }
    Ok(())
}

/// Tracks that each `inner_proof` binding is used by exactly one
/// `prepare_verification`, under the same guard.
///
/// Guards must match because each instruction reads only its own: guarded
/// off, `inner_proof` binds an empty blob that a `prepare_verification` guarded on
/// would then fail to verify.
#[derive(Default)]
struct InnerProofUses<'a> {
    bound: HashSet<&'a Identifier>,
    /// Bound but not yet verified, with their guard, in instruction order.
    pending: Vec<(&'a Identifier, &'a Operand)>,
}

impl<'a> InnerProofUses<'a> {
    /// Records an `inner_proof` binding.
    fn bind(&mut self, id: &'a Identifier, guard: &'a Operand) -> anyhow::Result<()> {
        if !self.bound.insert(id) {
            bail!("`inner_proof` rebinds {}", id.0);
        }
        self.pending.push((id, guard));
        Ok(())
    }

    /// Records the `prepare_verification` of a bound proof.
    fn verify(&mut self, proof: &Identifier, guard: &Operand) -> anyhow::Result<()> {
        let Some(pos) = self.pending.iter().position(|(id, _)| *id == proof) else {
            if self.bound.contains(proof) {
                bail!("{} is verified more than once", proof.0);
            }
            bail!(
                "`prepare_verification` names {}, which no preceding `inner_proof` binds",
                proof.0
            );
        };
        let (_, bound_guard) = self.pending.remove(pos);
        if bound_guard != guard {
            bail!(
                "`prepare_verification` on {} is guarded differently to the `inner_proof` that bound it",
                proof.0
            );
        }
        Ok(())
    }

    /// Rejects a bound proof left unverified.
    fn finish(&self) -> anyhow::Result<()> {
        if let Some((id, _)) = self.pending.first() {
            bail!(
                "`inner_proof` binds {}, which no `prepare_verification` uses",
                id.0
            );
        }
        Ok(())
    }
}

impl IrSource {
    /// Rejects a malformed `inner_proof` / `prepare_verification` pair: each
    /// `prepare_verification` must name a proof an earlier `inner_proof` bound, under
    /// the same guard, and each bound proof must be used exactly once.
    pub(crate) fn validate_inner_proofs(&self) -> anyhow::Result<()> {
        let mut proofs = InnerProofUses::default();
        for ins in self.instructions.iter() {
            match ins {
                I::InnerProof { guard, output } => proofs.bind(output, guard)?,
                I::PrepareVerification { guard, proof, .. } => proofs.verify(proof, guard)?,
                _ => {}
            }
        }
        proofs.finish()
    }

    /// Indexes [`IrSource::inner_vks`] by digest, so each `PrepareVerification`
    /// can resolve its key by `vk_hash`.
    ///
    /// Rejects a duplicated blob, and one left unused by every instruction, so
    /// the side-table stays a canonical set of the keys the circuit needs.
    pub(crate) fn resolve_inner_vks(&self) -> anyhow::Result<HashMap<Vec<u8>, &[u8]>> {
        let mut vk_map: HashMap<Vec<u8>, &[u8]> = HashMap::new();
        for vk_blob in self.inner_vks.iter() {
            let vk_hash = Sha256::digest(vk_blob).to_vec();
            if vk_map.insert(vk_hash, vk_blob).is_some() {
                bail!("duplicate verifying key in `inner_vks`");
            }
        }

        let mut used = HashSet::new();
        for ins in self.instructions.iter() {
            if let I::PrepareVerification { vk_hash, .. } = ins {
                if !vk_map.contains_key(vk_hash) {
                    bail!(
                        "no verifying key in `inner_vks` for vk_hash 0x{}",
                        const_hex::encode(vk_hash)
                    );
                }
                used.insert(vk_hash);
            }
        }

        if used.len() != vk_map.len() {
            bail!(
                "`inner_vks` holds {} keys but only {} are used",
                vk_map.len(),
                used.len()
            );
        }
        Ok(vk_map)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::BufReader;

    use midnight_curves::G2Projective;
    use transient_crypto::proofs::ParamsProver;

    use super::*;

    /// Pins [`SRS_S_G2`] to the SRS the ledger verifies against, and pins the
    /// assumption that lets one point stand for the whole of it.
    #[test]
    fn embedded_s_g2_matches_the_midnight_srs() {
        // Skips rather than fails to build without the SRS cache;
        // `inner_proof_artifacts.rs` pins `SRS_VERIFIER` without it.
        let Some(dir) = option_env!("MIDNIGHT_PP") else {
            eprintln!("MIDNIGHT_PP is unset: skipping the SRS comparison");
            return;
        };
        let params = ParamsProver::read(BufReader::new(
            File::open(format!("{dir}/bls_midnight_2p14")).expect("the Midnight SRS cache"),
        ))
        .expect("the SRS parses");

        assert_eq!(
            params.0.g2(),
            G2Projective::generator(),
            "`ParamsVerifierKZG::read` derives its negated generator rather than reading `g2`"
        );
        assert_eq!(params.0.verifier_params().s_g2(), SRS_VERIFIER.s_g2());
    }
}
