// SPDX-License-Identifier: MIT
// The witness's hash on the page's side, so a check can compare what its tap read with
// what the mix handed over: FNV-1a 64 over the samples' little-endian bytes, as one
// stream, the same as `crates/sound-mix/src/witness.rs`. It walks BigInts, so it is for a
// check's samples and never for a frame's time budget.
//
// Call: hashSamples(HASH_START, int16s) -> the hash, a BigInt; hand that back as the
// first argument to go on. hashText(hash) -> 16 hex digits, as the Rust side prints it.
const PRIME = 0x0000_0100_0000_01b3n;
const MASK = 0xffffffffffffffffn;

export const HASH_START = 0xcbf29ce484222325n;

export function hashSamples(from, samples) {
  let hash = from;
  for (const sample of samples) {
    hash = ((hash ^ BigInt(sample & 0xff)) * PRIME) & MASK;
    hash = ((hash ^ BigInt((sample >> 8) & 0xff)) * PRIME) & MASK;
  }
  return hash;
}

export function hashText(hash) {
  return hash.toString(16).padStart(16, "0");
}
