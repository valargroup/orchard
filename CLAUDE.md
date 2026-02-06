# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Orchard is a Rust implementation of the Zcash Orchard shielded transaction protocol. It provides privacy-preserving transaction functionality using the Halo 2 zero-knowledge proving system. The crate supports `no_std` environments (WASM, ARM Cortex-M).

## Build & Test Commands

```bash
cargo build --all-features          # Full build
cargo test --all-features           # Run all tests
cargo fmt -- --check                # Check formatting
cargo clippy -- -D warnings         # Lint (CI denies all warnings)
cargo doc --all-features --document-private-items  # Build docs, checks intra-doc links
cargo bench                         # Run benchmarks (criterion)
```

**Running a single test:**
```bash
cargo test <test_name>              # By test function name
cargo test --test builder           # Single integration test file
cargo test --lib circuit            # Tests in a specific module
```

**Toolchain:** Rust 1.70.0 (pinned in `rust-toolchain.toml`). Components: clippy, rustfmt.

## Feature Flags

- `circuit` (default) — Halo 2 circuit/proof support
- `multicore` (default) — Parallel proving
- `std` (default) — Standard library; disable for `no_std`
- `test-dependencies` — Proptest generators for downstream testing
- `unstable-frost` — Experimental FROST threshold signing
- `dev-graph` — Circuit visualization with plotters

## Architecture

**Transaction flow:** Keys → Notes → Actions → Bundles → Proofs

- **`keys.rs`** — Full key hierarchy: `SpendingKey` → `SpendAuthorizingKey` → `FullViewingKey` → `IncomingViewingKey`. ZIP 32 hierarchical derivation in `zip32.rs`.
- **`note.rs`** — Note structure (value, address, rho, psi, rcm). Sub-modules for commitment and nullifier derivation.
- **`action.rs`** — Atomic unit combining one spend + one output. Contains nullifier, commitment, encrypted note, value commitment.
- **`bundle.rs`** — Collection of actions with a unified Halo 2 proof and binding signature. `batch.rs` handles batch verification.
- **`builder.rs`** — High-level API for constructing transactions. Manages note selection, padding, randomization, and value balance.
- **`circuit.rs`** — Main Halo 2 circuit (K=11). Sub-circuits: `blake2b.rs` (hash function), `commit_ivk.rs` (viewing key commitment), `note_commit.rs` (note commitment), `gadget/add_chip.rs` (field arithmetic).
- **`spec.rs`** — Direct implementations of Zcash protocol specification functions.
- **`note_encryption.rs`** — Note encryption/decryption using key agreement.
- **`pczt.rs`** — Partially-Created Zcash Transaction workflow for distributed signing (parse → update → prove → sign → extract).
- **`tree.rs`** — Incremental Merkle tree for note commitments and anchor verification.
- **`value.rs`** — Value commitments with homomorphic properties for balance verification.
- **`constants.rs`** — Fixed elliptic curve base points, Sinsemilla parameters, protocol constants.

## Key Dependencies

- **`halo2_proofs`/`halo2_gadgets`** — Zero-knowledge proving system and circuit gadgets (ECC, Poseidon, Sinsemilla, Merkle chips)
- **`pasta_curves`** — Pallas/Vesta curve arithmetic
- **`reddsa`** — RedPallas spend authorization signatures
- **`zcash_note_encryption`** — Note encryption trait implementations
- **`zcash_spec`** — Zcash protocol specification helpers

## Circuit Auditing

When asked to audit a circuit, use `docs/audits/zcash-nu5-qedit-audit.md` as the gold standard reference. It demonstrates the expected methodology: per-component verification of constraints, variable connectivity, range checks, and integrity of cryptographic operations (nullifier derivation, note commitments, value commitments, spend authority, Merkle paths). Findings should be categorized as Critical, Warning, Minor, or No Issues Found, with clear descriptions, code references, and recommendations.

## Branch Naming

Use `adam/` as the branch name prefix when creating branches or PRs.
