# ZKIR 3.2.1 Release Notes

**Version:** 3.2.1
**Date:** 2026-11-11

## High-level summary

ZKIR 3.2.1 is a bug-fix release for ZKIR 3.2.0. It fixes chip-architecture
detection (`used_chips`): the architecture is now derived from a type
inference pass over the whole circuit, instead of inspecting only the
circuit inputs and a few entry-point instructions. Previously, a type that
entered the circuit only through another instruction — for example a
`LoadConstant` of a foreign-field constant, an `EcMulGenerator` output, or
`JubjubScalarFromNative` — did not enable the chip it needs, and key
generation and proving failed for such circuits. This affected circuits
emitted by the Compact compiler.

There are no IR language changes: ZKIR 3.0, 3.1 and 3.2 IR loads exactly as
in 3.2.0. Circuits that worked with 3.2.0 select the same chip architecture,
so existing proving and verifying keys remain valid.

## Audience

This release note is important for:

- Compact compiler developers emitting ZKIR 3.x
- Developers using the ZKIR WASM bindings (`@midnightntwrk/zkir-v3`)
- Rust consumers of the `midnight-zkir` crate

## Summary of updates

### Proving backend updates

- `used_chips` derives the chip architecture from a new types pass that
  infers the type of every variable in the circuit, so every instruction —
  not just the entry points — enables the chips it requires.
- In debug builds, `preprocess` cross-checks the types pass against the
  types of the actually computed values.

## New features requiring configuration updates

*None in this release.*

## Deprecations

*None in this release.*

## Breaking changes or required actions for developers

*None in this release.* Keys generated with ZKIR 3.2.0 for circuits that
worked remain valid; circuits that previously failed chip detection work
after regenerating their keys with 3.2.1.

## Fixed defect list

| **Component** | **Description** |
|---------------|-----------------|
| zkir (chip detection) | `used_chips` missed types introduced only by non-entry-point instructions (e.g. `LoadConstant`, `EcMulGenerator`, `JubjubScalarFromNative`), so the required chip was not enabled and key generation/proving failed for such circuits. |
