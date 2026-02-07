# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Orchard is a Rust implementation of the Zcash Orchard shielded transaction protocol. It provides privacy-preserving transaction functionality using the Halo 2 zero-knowledge proving system. The crate supports `no_std` environments (WASM, ARM Cortex-M).

## Protocol Documentation Reference

**Always consult the documentation files first** — Claude's training data may lack sufficient detail on these specific protocols.

### Zcash Protocol Spec

- `docs/papers/zcash-protocol-index.md` - Section index with line ranges (read this FIRST)
- `docs/papers/zcash-protocol.tex` - Full protocol spec in LaTeX (read specific line ranges from index)

## Circuit Auditing

When asked to audit a circuit, use `docs/audits/zcash-nu5-qedit-audit.md` as the gold standard reference. It demonstrates the expected methodology: per-component verification of constraints, variable connectivity, range checks, and integrity of cryptographic operations (nullifier derivation, note commitments, value commitments, spend authority, Merkle paths). Findings should be categorized as Critical, Warning, Minor, or No Issues Found, with clear descriptions, code references, and recommendations.

## Branch Naming

Use `adam/` as the branch name prefix when creating branches or PRs.
