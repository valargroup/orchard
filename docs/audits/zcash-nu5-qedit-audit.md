![](https://i.imgur.com/PyNeb3o.png =250x100)

# Zcash NU5 Final Audit Report - September 2021 

[TOC]

## Overview

This document presents the findings of the [QEDIT team](https://qed-it.com/about-us/) during the review of the [Zcash NU5](https://zips.z.cash/zip-0252) protocol update. Specifically the review is focused on (1) the [Halo 2 proving system](https://electriccoin.co/blog/explaining-halo-2/) with its latest PLONK-style arithmetization for representing constraint systems; (2) the [Orchard protocol](https://zips.z.cash/zip-0224), which, even if based on Sapling, differs considerably from it; (3) the underlying primitives of Orchard, from hash functions and commitments to signature schemes; (4) the latest key structure and derivation system; (5) the underlying field and elliptic curve arithmetic and parameter generation; and (6) the actual circuit implementation that describes the Orchard Action statement. Note that we explicitly exclude any blockchain-related component not associated with orchard or its interface (consensus, network, block generation, etc.)

For this review, we evaluated the specification of the protocol design for security and audited the implementation for vulnerabilities that could affect the system as a whole.

This review spanned the period of ~5 months, intermitently, from April 21st until September 24th, 2021. 

### Resources used in the review

In this section we include the resources used to conduct the review of NU5, including specification, documentation, implementations, test vectors and individual ZIPs.

**Specification, Design & Security**

- [The Orchard / NU5 specification](https://zips.z.cash/protocol/nu5.pdf), for which the low-level details of Orchard, its primitives, security requirements, encodings and more are best explained.
- [Halo2 Book](https://zcash.github.io/halo2/index.html), for which the Halo 2 construction and PLONKish arithmetization are best explained.
- [Orchard Book](https://zcash.github.io/orchard/index.html), for which the explanation of nullifier security, and high-level orchard protocol is best explained
- [Ariel Gabizon's proof of security for Sapling](https://github.com/zcash/sapling-security-analysis)
- Other proofs of security: Appendix B of [Decentralized Anonymous Micropayments](https://eprint.iacr.org/2016/1033.pdf)
- Proof of random permutations for F4Jumble by Gentry and Ramzan on [Eliminating Random Permutation Oracles in the Even-Mansour Cipher](https://iacr.org/archive/asiacrypt2004/33290031/33290031.pdf)


**Zcash Improvement Proposals (ZIPs)** 
This is a list of all [the ZIPs included in NU5 upgrade](https://zips.z.cash/#nu5-zips), of which we reviewed both in design and in code the following:
- [ZIP 224: Orchard Shielded Protocol](https://zips.z.cash/zip-0224)
- [ZIP 244: Transaction Identifier Non-Malleability](https://zips.z.cash/zip-0244)
- [ZIP 032: Shielded Hierarchical Deterministic Wallets](https://zips.z.cash/zip-0032)
- [ZIP 316: Unified Addresses and Unified Viewing Keys](https://zips.z.cash/zip-0316)


**Code Bases & Commits**

For the libraries and code components, we include the specific commits that have been reviewed:

- The [halo2](https://github.com/zcash/halo2/blob/7774dd8235298938bdbc519708d58ed06822098e/src) repository, including the following commits:
    - [ZK changes](https://github.com/zcash/halo2/pull/316)
    - [The Halo2 book](https://github.com/zcash/halo2/pull/320)
- The [Orchard](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src) repository, containing both the high-level protocol, the primitives and the circuit, including the following commits:
    - [Merkle path](https://github.com/zcash/orchard/pull/98) and [here](https://github.com/zcash/orchard/pull/136)
    - [ECC mul](https://github.com/zcash/orchard/pull/54), [here](https://github.com/zcash/orchard/pull/111) (circuit chip for ECCmul) and [here](https://github.com/zcash/orchard/pull/145)
    - [SinsemillaCommit](https://github.com/zcash/orchard/pull/118)
    - [halo2 API changes](https://github.com/zcash/orchard/pull/144), and [here](https://github.com/zcash/orchard/pull/150)
    - [Poseidon gadget](https://github.com/zcash/orchard/pull/28)
    - ECC gadget: [PR #73, commit: Optimized ECC chip](https://github.com/zcash/orchard/pull/73)
    - Sinsemilla gadget: [PR #67, commit: Sinsemilla chip with HashDomain](https://github.com/zcash/orchard/pull/67)
- The [pasta_curves](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/src) repository, containing all the elliptic curve cryptography used in Halo 2 and Orchard.
- The [librustzcash]() repository, containing the interface to the zcashd API and consensus rules, including the following commits:
    - [Transaction ID non-malleability](https://github.com/zcash/librustzcash/commit/3915abd0a1ee5b7e757c3fec0509d71f5e08fdfb)
    - [Unified Addresses & Unified Viewing Keys PR #352](https://github.com/zcash/librustzcash/pull/352)
    - [F4Jumble](https://github.com/zcash/librustzcash/blob/master/zcash_primitives/src/address/f4jumble.rs)

The following is a table that describes what reviewed components are included in each of the repositories. 

![](https://i.imgur.com/jQrseqC.jpg)


## Summary of Findings

Most importantly, **we did not find any critical issues** that could affect the well being of the deployed system or the funds, anonymity or confidentiality of the Zcash users. Only a few of the issues that are reported were found to be exploitable under (preventable) specific circumstances (such as [QZ2-305](#[QZ2-305]-UA-address-prioritization-not-enforceable-on-chain)).

In general the protocol is well specified and the code is mostly well written. Although the circuit generating code is well documented, many other components are not well documented. Overall the implementation is in accordance with the detailed specification.

It is important to note that reviewing the security of such a complex system and its implementation is a challenge. Mainly, when trying to ensure the privacy and integrity of the Zcash blockchain, one needs to analyze different layers, both individually and in conjuction with one another. Integrity is provided by the soundness of the proving system and privacy is provided by the zero-knowledge property of the proving system. Both, however, depend on the robustness of the circuit design and implementation and the underlying security of the cryptographic primitives used. For example:
- If any of the required constraints are missing (such as checking that a variable is zero), it could enable malicious actors to steal or generate illegitimate funds.
- If any of the circuit variables are not properly referenced in each of the repeated uses, one could generate a proof of a non-verifying statement.
- If the signature scheme is broken (e.g.: DDH is broken), then malicious actors could potentially spend your funds (especially if they have the viewing keys).

Privacy, however, is especially tricky since data leaked on-chain or via the peer-to-peer network is persistent and can be analyzed retroactively. Here is a (non-exhaustive) list of components that need to be reviewed as they affect the overall privacy of the system:
- No secret witness data should be leaked into the ZKP instance or transaction records.
- The Halo 2 scheme and its implementation achieve zero-knowledge (given a good randomness source).
- The source of randomness used across the system is uniform and never reset or reused.
- Primitives (commitments, hashes, signatures) are secure and properly implemented (e.g.: if commitments / transactions could be linked, the whole transaction graph would be recovered).
- Elliptic curves are properly generated and implemented, according to a secure instantiation.
- There is proper canonization of all transmitted values such that there is a unique representation, and unique encoding in the circuit code.
    - Example: ensuring modular reduction for finite field values
    - Example: unique EC point representation
- Leakage via the number of actions could compromise the privacy of the system, but this was out of scope for this audit.
    - Example: dust attack where sending many small payments to an address may induce sends from that address to have abnormally many actions
- Side channel attacks could compromise the privacy of the system, but this was out of scope for this audit.

***EDIT 29/09/2021:** The ECC team found a warning issue related to checking non-zero values of specific witnessed points, as per the requirement outlined in issue [QZ2-514](#[QZ2-514]-Diversified-address-integrity-check). See the [official release announcement](https://electriccoin.co/blog/new-release-4-5-1/).*


### Legend and stats
In this audit, we report on four types of findings, each of which is identified by a unique identifier, e.g.: [QZ1-205] implies an issue reported in Phase 1 "QZ1" and it is the 5th issue found in subsection 2 (Orchard Primitives) of this section.

:::danger
In red we describe **Critical Issues**, findings that we have demonstrated can be exploited to attack the Zcash system.
:::

We have not found any critical issues in this review.

:::warning
In yellow we describe **Warning Issues**, findings that we believe could result in exploits, but their impact on the system have not been demonstrated.
:::

We have found a total of 7 warning issues:
- [QZ1-001](#[QZ1-001]-Wrong-definition-of-Point-at-Infinity)
- [QZ2-108](#[QZ2-108]-Ordering-of-Instance-/-Advice-/-Fixed-variable-is-forced-in-the-code,-not-in-the-spec)
- [QZ1-201](#[QZ1-201]-Hash-To-Field)
- [QZ1-301](#[QZ1-301]-Tests-are-not-triggered-in-keys.rs-and-other-files.)
- [QZ1-302](#[QZ1-302]-Numeric-domain-separators-are-used-directly.-#1-#2-#3-#4)
- [QZ2-305](#[QZ2-305]-UA-address-prioritization-not-enforceable-on-chain)
- [QZ2-410](#[QZ2-410]-Randomness-reuse-in-restored-virtual-machines)

:::info
In blue we describe **Minor Issues**, findings that have no meaningful impact on the system but should be fixed, such as typos, spec <-> code inconsistencies, styling and naming conventions, potential optimizations, etc.)
:::

We have found a total of 19 minor issues:
- [QZ1-002](#[QZ1-002]-Optimization:-Pasta-Curves-Implementation-ct_eq)
- [QZ1-103](#[QZ1-103]-Isogeny-mapping-at-infinity)
- [QZ1-104](#[QZ1-104]-Typo-in-Halo2-book)
- [QZ1-105](#[QZ1-105]-Optimization-in-Polynomial-Evaluation)
- [QZ1-106](#[QZ1-106]-Pasta-Curves-Implementation)
- [QZ1-107](#[QZ1-107]-Typo-in-specification-§5.4.9.6-on-Pallas-and-Vesta)
- [QZ2-109](#[QZ2-109]-Naming-of-variable-error-prone-[Halo2:-circuit.rs::replace_selector)
- [QZ2-110](#[QZ2-110]-Complex-function-should-be-splitted,-and-simplified-[Halo2:-circuit.rs::compress_selector)
- [QZ2-112](#[QZ2-112]-Code-Readability-[Halo2:-Lookup-arguments)
- [QZ1-202](#[QZ1-202]-Hash-To-Field)
- [QZ1-203](#[QZ1-203]-Security-Comment-on-Sinsemilla-Instantiation-§5.4.8.4)
- [QZ1-401](#[QZ1-401]-Lack-of-Orchard-Shielded-Protocol-Proof-of-Security)
- [QZ1-402](#[QZ1-402]-Nullifier-Generation-Clarification-Needed-in-the-specification-§4.16)
- [QZ2-409](#[QZ2-409]-RNG-and-source-of-entropy-on-virtual-machine-(VM)-in-the-*Cloud*)
- [QZ2-501](#[QZ2-501]-Naming-style-and-convention-of-gate-definition-and-usage)
- [QZ2-502](#[QZ2-502]-Naming-style-for-regions-and-columns)
- [QZ2-503](#[QZ2-503]-Unfixed-offset-for-the-instance-column-count)
- [QZ2-504](#[QZ2-504]-Minor-inefficiency-in-cloning-patterns)
- [QZ2-505](#[QZ2-505]-note_commit_config-region-assignment)

:::success
In green we describe **No Issues Found**, positive findings worth reporting.
:::

We have reported a total of 37 issues with a successful review.

Note that by the time of publishing, some of the issues found may have already been fixed by the ECC development team.

## Fields & Elliptic Curves Libraries [ID QZ*-0**]

:::warning
### [QZ1-001] [Wrong definition of Point at Infinity](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/src/curves.rs#L866)

- **Description:** In the _new_curve_impl_ macro, the point at infinity is defined using the following function as:
```rust=86
fn identity() -> Self {
    Self {
        x: $base::zero(),
        y: $base::zero(),
        z: $base::zero(),
    }
}
```
This is incorrect from the cryptographic point-of-view, because in Jacobian coordinates, the point at infinity should be defined as $(t^2, t^3, 0)$ for any $t \neq 0$. The "usual" representant used is $(1, 1, 0)$. Nevertheless, from an engineering point-of-view, it's correct provided that wherever it could be  used (or computed) proper checks are performed. 
- **Recommendation:**
  Replace the function implementation by the following one:
```rust
fn identity() -> Self {
    Self {
        x: $base::one(),
        y: $base::one(),
        z: $base::zero(),
    }
}
```
- **Rationale:**
  Using the correct definition of the point at infinity allows to implement EC operations as usual, with no specific check for this case (checks can still be done for optimization purposes though).
:::

:::info
### [QZ1-002] [Optimization: Pasta Curves Implementation ct_eq](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/curves.rs#L272)
- **Description:** Equality testing for EC points can be optimized
Currently, the implementation "normalizes" points to affine coordinates. A first test is done to know if they are points at infinity or not. Then, it returns the following boolean:
```rust
(self_is_zero & other_is_zero) // Both point at infinity
    | ((!self_is_zero) & (!other_is_zero) & x1.ct_eq(&x2) & y1.ct_eq(&y2))
    // Neither point at infinity, coordinates are the same
```
- **Recommendation:**
  Provided that [QZ1-001] is implemented, this could be optimized by only keeping the normalization + comparison, and remove all _is_zero_ tests.
:::

:::success
### [QZ1-003] [Implementation of Field arithmetic](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/arithmetic/fields.rs)
- **Description:** 
  Implementation of low-level functions (_adc_, _sbb_, _mac_), and modular functions have been checked, and tested against test vectors generated using PARI-GP.
- No issue found.
:::

:::success
### [QZ1-004] [Square root computation using Sarkar algorithm](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/arithmetic/fields.rs)
- **Description:** 
  Square root computation using Sarkar2020 has been checked for consistency and correctness. It has been checked against test vectors using PARI-GP.
- No issue found.
:::

:::success
### [QZ1-005] [Operations on elliptic curves](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/curves.rs)
- **Description:** 
The macro _new_curve_impl_ and all sub-macros have been looked at carefully, to ensure the proper implementation of elliptic curves operations. These functions have been tested against a PARI-GP implementation for both Jacobian and Affine coordinates.
- No issue found.
:::

:::success
### [QZ1-006] [Import/Export of ECPoints / Field Elements](https://github.com/zcash/pasta_curves)
- **Description:** Implementation of the import/export functions of field elements/ECPoints from/to binary buffers are correct, and match the specification. The same attention has been paid to the point compression techniques.
- No issue found.
:::



:::success
### [QZ1-007] [Constant-time EC comparison](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/curves.rs#L257)
- **Description:** 
Benchmarks have been implemented to check whether the implementation is indeed constant time, with the following code.
```rust
fn bench_test_points_equality(c: &mut Criterion) {
    use rand::rngs::ThreadRng;

    let mut group = c.benchmark_group("ct-points-equality");

    // Initialize the benchmark data.
    let not_zero = (0..100000).into_iter().map(|_| Point::random(ThreadRng::default())).collect::<Vec<Point>>();
    let only_zero = (0..100000).into_iter().map(|_| Point::identity()).collect::<Vec<Point>>();

    // Let the bencher compute the average execution time.
    group.bench_function("not-null", |b| b.iter(|| check_point_equality_vectors(&not_zero, &not_zero)));
    group.bench_function("one-null", |b| b.iter(|| check_point_equality_vectors(&not_zero, &only_zero)));
    group.bench_function("only-null", |b| b.iter(|| check_point_equality_vectors(&only_zero, &only_zero)));

}
```
- Results are good, and differences are considered in the noise threshold:
```
points-equality/not-null                                                                            
                        time:   [28.527 ms 28.552 ms 28.582 ms]
points-equality/one-null                                                                            
                        time:   [28.358 ms 28.372 ms 28.388 ms]
points-equality/only-null                                                                            
                        time:   [27.902 ms 27.968 ms 28.044 ms]
```
:::

:::success
### [QZ1-008] [Pallas / Vesta Curve parameters](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/curves.rs#L898)
- **Description:** Parameters for the Pallas & Vesta curves have been checked, tested for primality, group order, generators, etc., and checked their correct implementation.
- No issue found.
- **NB**: No security review has been done on the Pasta parameters / generation to ensure that they can be considered as a DLP-secure group with the desired level of security (as required for the Halo 2 proving system). 
:::

---------------------------------------------------------------------------------------------
## Halo 2 [ID QZ*-1**]

:::success
### [QZ1-101] [Pasta Curves Implementation](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/src/curves.rs#L898)
- **Description:** Implementation of Pasta curves and their isogeneous curves match the specification.
  Careful attention has been paid to check that all the constants involved were indeed matching the specification.
- No issue found.
:::

:::success
### [QZ1-102] [Isogeny Mapping](https://github.com/zcash/pasta_curves/blob/a1194672c55ee03a1631b6e1af9d86a3bd87ab70/src/hashtocurve.rs#L80)
- **Description:** Implementation of _isogeny mapping_ matches the specification.
- No issue found.
:::

:::info
### [QZ1-103] [Isogeny mapping at infinity](https://github.com/zcash/pasta_curves/blob/a1194672c55ee03a1631b6e1af9d86a3bd87ab70/src/hashtocurve.rs#L80)
- **Description:** Applying the isogeny mapping to the point at infinity should output the point at infinity on the output curve. 
In the current algorithm, even if the implementation matches the specification, when applied to the point at infinity $(1, 1, 0)$ (in Jacobian coordinates), it gives $(0, 0, 0)$. 
- **Recommendation:**  This is a strange behaviour, and the original algorithm should be reviewed for completeness.

:::

:::info
### [QZ1-104] [Typo in Halo2 book](https://zcash.github.io/halo2/user/tips-and-tricks.html)
- **Description:** 
A simple typo has been found in the Lagrange interpolation's formula:
$$
l_j(X)= \prod_{0 \leq m\lt k, m \neq j} \frac{x - x_m}{x_j - x_m}
$$

while it should be:

$$
l_j(X)= \prod_{0 \leq m\lt k, m \neq j} \frac{X - x_m}{x_j - x_m}
$$

Note the capital $X$ in the numerator.
- **Recommendation:**
  Replace it in the formula.
:::


:::info
### [QZ1-105] [Optimization in Polynomial Evaluation](https://github.com/zcash/halo2/blob/7774dd8235298938bdbc519708d58ed06822098e/src/arithmetic.rs#L305)
- **Description:** The current implementation computes at each step the value $X^k$ as `cur *= &point;` then computes at the next step the element $a_kX^k$ and add it to the _accumulator_ as `acc += &(cur * coeff);`.
```rust=305
pub fn eval_polynomial<F: Field>(poly: &[F], point: F) -> F {
    // TODO: parallelize?
    let mut acc = F::zero();
    let mut cur = F::one();
    for coeff in poly {
        acc += &(cur * coeff);
        cur *= &point;
    }
    acc
}
```
- **Recommendation:**
In the serial implementation, this can be optimized by using the Horner's method to evaluate polynomials: $a_kX^k + a_{k-1}X^{k-1} + \cdots + a_0 = (((0 + a_k)X + a_{k-1})X + \cdots + )X + a_0$.
```rust
pub fn eval_polynomial<F: Field>(poly: &[F], point: F) -> F {
    let mut acc = F::zero();
    for coeff in poly.iter().rev() {
        acc *= &point;
        acc += coeff;
    }
    acc
}
```
One can see that this code requires one less multiplication per step.
__NB__:  this optimization is still feasible in a parallelized way using a tree structure (particularly efficient if the degree of the polynomial is a power of 2, which is the case in polynomials used here).
:::

:::info
### [QZ1-106] [Pasta Curves Implementation](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/curves.rs#L597)
- **Description:** Use of hardcoded type in trait function implementation.
```rust=597
        impl GroupEncoding for $name_affine {
            type Repr = [u8; 32];

            fn from_bytes(bytes: &[u8; 32]) -> CtOption<Self> {
```
```rust=631
            fn to_bytes(&self) -> [u8; 32] {
```
- **Recommendation:**
Use the type `Repr` defined for the trait.
```rust=597
        impl GroupEncoding for $name_affine {
            type Repr = [u8; 32];

            fn from_bytes(bytes: &Repr) -> CtOption<Self> {^
```
```rust=631
            fn to_bytes(&self) -> Repr {
```
:::

:::info
### [QZ1-107] Typo in specification §5.4.2 on computing OVK

- **Description:** There is a typo in the use of `BLAKE2b-256` versus `BLAKE2b-512` as seen below
![](https://i.imgur.com/KTkXaUh.png)

- **Recommendation:** Fix it.
:::

:::warning
### [QZ2-108] Ordering of Instance / Advice / Fixed variable is forced in the code, not in the spec

- **Description:** halo2/plonk/circuit.rs l.45 forces it to Advice < Instance < Fixed
  This may break interoperability if another implementation uses a different convention.
- **Recommendation:** Force it in the spec.
:::

:::info
### [QZ2-109] Naming of variable error prone [halo2: circuit.rs::replace_selector](https://github.com/zcash/halo2/blob/27c4187673a9c6ade13fbdbd4f20955530c22d7f/src/plonk/circuit.rs#L1294)

- **Description:** 
  The boolean used if called *must_be_nonsimple* while everywhere else "non-simple" selector are called "complex selector".
- **Recommendation:** Replace the name by *must_be_complex*
:::

:::info
### [QZ2-110] Complex function should be split, and simplified [halo2: circuit.rs::compress_selector](https://github.com/zcash/halo2/blob/27c4187673a9c6ade13fbdbd4f20955530c22d7f/src/plonk/circuit.rs#L1294)

- **Description:** 
  This function is complex (+150 lines), and could be simplified.
For example, this possible simplification has been identified when creating the *selector_map* and *selector_replacements* variables:
```rust=1275
let mut selector_map = vec![None; selector_assignment.len()];
let mut selector_replacements = vec![None; selector_assignment.len()];
for assignment in selector_assignment {
    selector_replacements[assignment.selector] = Some(assignment.expression);
    selector_map[assignment.selector] = Some(new_columns[assignment.combination_index]);
}

self.selector_map = selector_map
    .into_iter()
    .map(|a| a.unwrap())
    .collect::<Vec<_>>();
let selector_replacements = selector_replacements
    .into_iter()
    .map(|a| a.unwrap())
    .collect::<Vec<_>>();
```
This code seems to first create a vector of `Option` and then iterates over each value to call `.unwrap()`.
- **Recommendation:** Simplify the code by removing useless operations.
:::


:::success
### [QZ2-111] Generation of Proving / Verifying Key [halo2: keygen.rs](https://github.com/zcash/halo2/blob/27c4187673a9c6ade13fbdbd4f20955530c22d7f/src/plonk/keygen.rs)
- **Description:** The generation of Proving/Verifying has been reviewed.
The implementation is straightforward, and follows the specification. Some optimizations were suggested in the code as comments.
- **No issue found**: Nevertheless, implementing optimizations suggested in the code would be good.
:::

:::info
### [QZ2-112] Code Readability [halo2: Lookup arguments](https://github.com/zcash/halo2/blob/74f3e1c6d9f79dae598e955fc8bcb251c3eff1b5/src/plonk/lookup.rs#L52)

- **Description:** 
  This rust code computes the *required degree* of an argument using its input/table polynomial expressions. This code is very naive and implies unnecessary for loops, while it could be done using Rust-specific optimizations. It may increase readability.
E.g:
```rust=52
let mut input_degree = 1;
for expr in self.input_expressions.iter() {
    input_degree = std::cmp::max(input_degree, expr.degree());
}
let mut table_degree = 1;
for expr in self.table_expressions.iter() {
    table_degree = std::cmp::max(table_degree, expr.degree());
}
```
This code could be rewritten using the `fold()` trick, like:
```rust=
let input_degree = self.input_expressions
    .iter()
    .fold(1, |accu, expr| std::cmp::max(accu, expr.degree()));
```
- **Recommendation:** Replace the name by *must_be_complex*
:::

:::success
### [QZ2-113] Polynomial commitment [halo2: Polynomial commitment](https://github.com/zcash/halo2/tree/209144981a35f88f3a9556434f1e39efa5767862/src/poly/commitment)

- **Description:** 
  The implementation of polynomial commitment (prove/verify) has been reviewed and matched against the Halo2 book. Implementation is straightforward and follows the spec.
- **No issue found**
:::

:::success
### [QZ2-114] Implementation of Halo 2 lookup argument  [halo2: Lookup Argument](https://github.com/zcash/halo2/tree/209144981a35f88f3a9556434f1e39efa5767862/src/plonk/lookup)

- **Description:** 
  The implementation of lookup argument (prove/verify) has been reviewed and matched against the Halo2 book. Implementation is straightforward and follows the spec.
- **No issue found**
:::

:::success
### [QZ2-115] Implementation of Plonk permutation argument  [halo2: Plonk permutation](https://github.com/zcash/halo2/tree/209144981a35f88f3a9556434f1e39efa5767862/src/plonk/permutation)

- **Description:** 
  The implementation of permutation argument (prove/verify) has been reviewed and matched against the Halo2 book. Implementation is straightforward and follows the spec.
- **No issue found**
:::

:::success
### [QZ2-116] Implementation of Plonk Vanishing argument  [halo2: Plonk vanishing](https://github.com/zcash/halo2/tree/209144981a35f88f3a9556434f1e39efa5767862/src/plonk/vanishing)

- **Description:** 
  The implementation of vanishing argument (prove/verify) has been reviewed and matched against the Halo2 book. Implementation is straightforward and follows the spec.
- **No issue found**
:::


---------------------------------------------------------------------------------------------
## Orchard Primitives [ID QZ*-2**]

![](https://i.imgur.com/cjSVetw.jpg)


This diagram represents the different components used in the PRFs, Hash functions and commitments deriving from the GroupHash functionality. A connecting line originates at the end with a diamond and lands on the "empty" end of the line. This means that the box at the end of the line is instantiated using the primitive in the box at the beginning of this line. There is no specific color coding, except that security requirements of connecting primitives are displayed in pink.

:::warning
### [QZ1-201] [Hash To Field](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/hashtocurve.rs#L10)
- **Description:** Function prototype does not match specification definition.
The function _hash_to_field_ uses `DST` in the spec, while DST is derived from the curve name, and the domain prefix. In the code, it directly requires the _domain_prefix_, and the curve name.
- **Recommendation:** Replace in the code the prototype of the function to take as inputs the DST instead of the domain prefix.
:::

:::info
### [QZ1-202] [Hash To Field](https://github.com/zcash/pasta_curves/blob/b016b972f8ef0eeff6b31e5dc01bccb833937dcf/main/src/hashtocurve.rs#L10)
- **Description:** Function implementation is error prone.
In the function _hash_to_field_, it is required to compute $b_0$, $b_1$, $b_2$, using DST. Instead of building DST once, and reusing it for the three computation, it is built three times.
- **Recommendation:** Build DST once using the needed parameters, and reuse it wherever it is required.
- **NB:** If the recommendation of **[QZ1-201]** is implemented, then this issue can be ignored.
:::

:::info
### [QZ1-203] Security Comment on Sinsemilla Instantiation [§5.4.8.4]()

- **Description:** In the instantiation of the `SinsemillaHashToPoint` function, an incomplete addition formula is used and there is a claim that _"the probability of returning `bottom` is insignificant"_, yet there is no further explanation, whereas it seems that the failure mode of even not checking the output properly can produce a non-trivial discrete logarithm relation, as proved in [Theorem 5.4.4](https://zips.z.cash/protocol/nu5.pdf#concretesinsemillahash)
- **Recommendation:** Further explain the solution.
:::

:::success 
### [QZ1-204] Security of $\mathsf{SinsemillaHash}$ and $\mathsf{SinsemillaHashToPoint}$ Functions [§5.4.1.9](https://zips.z.cash/protocol/nu5.pdf#concretesinsemillahash)

- **Description:** The $\mathsf{SinsemillaHash}$ (which in turn is instantiated using $\mathsf{SinsemillaHashToPoint}$ function) replaces the Bowe-Hopwood Perdersen hashes and is used in three main components in the protocol:
    - $\mathsf{MerkleCRH}$ to compute the tree path elements, which requires collision resistance 
    - $\mathsf{NoteCommit}$ to generate the hiding and binding commitment to be added as a leaf of the Merkle tree
    - $\mathsf{Commit}_{ivk}$ to derive the incoming viewing key
- The main requirement for the function's security is to be collision-resistant between inputs of fixed-length. Theorem 5.4.3 proves this is the case where $\mathsf{GroupHash}$ acts as a random oracle. The hiding property in the note commitment is derived from the pseudo-randomness provided by the $\mathsf{GroupHash}$ component.
- **No issues found.**
:::


:::success
### [QZ1-205] RedPallas Security Instantiation [§5.4.7](https://zips.z.cash/protocol/nu5.pdf#concretereddsa)

- **Descriptions:** 
    - [RedPallas `SpendAuthSig` Group Generator] The `GroupHash` function is used to generate the base generator for the RedPallas signature scheme, and can be instantiated as a pseudorandom generation as `GroupHash` can be used as a random oracle. This prevents others from generating a second dependent generator of the group and its discrete log, as expected in the scheme.
    - [RedPallas `BindingSig` re-randomization] The re-randomization algorithms, both for key generation and signing/verifying are accurately described in the specification, including bit encodings and endianness. The arithmetic of the key-homomorphic property is correct for the instantiation in `BindingSig`.
- **No issues found.**
- **Recommendation:** Explain further the security requirements and how they are instantiated for RedPallas in Orchard.
:::


:::success
### [QZ1-206] GroupHash [§5.4.9.8](https://zips.z.cash/protocol/nu5.pdf#concretegrouphashpallasandvesta)
- **Description:** The `GroupHash` function is used widely across the Orchard protocol to generate random-like ellitpic curve group elements, making it one of the most sensistive pieces of the system. In most cases the `GroupHash` must be used as a random oracle to produce safe group elements and independent generators. It is instantiated by using the Blake2s hash function
- It has been implemented as described in the [IETF hash-to-curve standard draft](https://www.ietf.org/archive/id/draft-irtf-cfrg-hash-to-curve-10.html#security-considerations) and in accordance to its security considerations.
- **No issues found.**
:::



---------------------------------------------------------------------------------------------
## Keys & Addresses [ID QZ*-3**]

![](https://i.imgur.com/SLJclfg.jpg)

The diagram above is a recreation of the typical key structure diagram in the spec, yet this diagram includes the type of variables, the functions invoked that map each of the variables, as well as a color code to show what is private / public. Here is a more concrete legend:
- Variable types
    - circles represent field elements / elliptic curve scalars
    - triangles represent byte strings
    - rectangles with round corners represent elliptic curve group elements
    - the parallelogram represents base field elements of the Pallas curve
- Color coding
    - red represents private elements, not shareable with anyone else
    - green represents public elements, shareable with all the network
    - yellow represents elements that must be shared with the receiver of the transfer
    - orange represents elements that can be shared with external parties but that imply a loss in privacy
    - blue represents intermediate computations

:::warning
### [QZ1-301] Tests are not triggered in [keys.rs](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src/keys.rs#L436) and other files.
- **Description:** A new strategy is defined using the `prop_compose` macro, but it is never triggered at the higher level.
Generally speaking, the code has low test coverage.
- **Recommendation:** Make sure the existing tests are being triggered.
Consider copying some of the test vectors from the Python implementation and encode them as tests in this repository.
:::

:::warning
### [QZ1-302] Numeric domain separators are used directly. [#1](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src/keys.rs#L76) [#2](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src/keys.rs#L146) [#3](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src/keys.rs#L166) [#4](https://github.com/zcash/orchard/blob/f82d00e40d982a0e49860afdeacc156b872268aa/src/keys.rs#L212)
- **Description:** The domain separators are being used as literals directly in a manner that makes it hard to understand the origin of the numbers without reading the spec. 
- **Recommendation:** Consider extracting the domain separators into named constants. 
:::

:::success
### [QZ2-303] Security of the derivation of keys and addresses
- **Description:** As shown in the diagram above, the keys and addresses have been restructured in Orchard, removing / unifying some of the keys previously used in Sapling. This issue alludes to the review done on the key derivations and their security
    - The viewing keys `ivk` and `ovk` provide visibility access to the transaction details (incoming and outgoing, respectively) and do not allow for spend authorization
    - The re-randomization process of `ak` and `ask` is secure under pre-image resistance, and collisions cannot be found
    - The nullifier key is unique and not derivable from other shareable keys
    - The Orchard addresses are properly diversified to prevent linkability across transactions, even from a sender or receiver to the transaction.
- **No issues found.**
:::

:::success
### [QZ2-304] [Unified Addresses design review](https://zips.z.cash/zip-0316)
- **Description:** Unified Addresses specify a standard way to convey several methods for payment to a Recipient's Wallet, based on any of the active Zcash protocols. The Sender's Wallet can then non-interactively select the method of payment, based on the protocol prioritization of the UA. We note that UAs are implemented as an external component to the main Zcash (Orchard, Sapling, Transparent) protocols. In fact, they only interface with the different protocols by using the address structures, but UAs are dealt with at the level of the third-party wallets. It is still important to review to ensure that no impactful malicious behaviour can be put in place.
    - The general design of the UA protocol is correct and secure.
    - Given the encoding and the invertibility of the F4Jumble, a UA can be correctly decoded by any third party wallet / user, and wrong addresses are ignored.
    - A given UA is unique to the prioritization of the protocol used as a form of payment. 
    - Same applies to the Unified Full Viewing Keys and Unified Incoming Viewing Keys.
- **No issues found.**
:::

:::warning
### [QZ2-305] UA address prioritization not enforceable on-chain
- **Description:** Given that the UA is never verified on-chain by consensus rules, the receiver of an Orchard, Sapling or transparent transfer does not have a *formal* way to verify that the protocol of UA prioritization was enforced.
    - A simple attack, that could break some privacy, would simply be that for a UA containing an Orchard address as first priority and a transparent address as second priority, then the sender could chose to send by transparent address, even if it supported Orchard. 
    - The UA is not on-chain, so no validation is done on the blockchain consensus level.
- **Recommendations:** As long as nothing about UAs is verified on-chain, it would be difficult to enforce properties. Even then, prioritization is very difficult to enforce - the UA protocol would have to be more robust. Note that this problem, at least the privacy leaking, is solved when transparent addresses are gone.
:::

:::success
### [QZ2-306] Near second pre-image collision of the encoding of Unified Addresses and Unified Viewing Keys
- **Description:** As described in the [address hardening](https://zips.z.cash/zip-0316#address-hardening) section of ZIP-316, one of the security goals is preventing a consumer of a UA from generating a collision UA, where one of the underlying addresses is either changed or removed completely
    - The UA is an approximation to a random permutation, which means that using the 4-round Feistel construction, a small change in the underlying address will generate a large change in the UA encoding, not allowing for a more efficient attack than brute-force.
    - We checked that the producer does not have a way to form a valid but not legal UA. Though there does not seem to be a meaningful attack, producers could not even find a partial collision between two UAs that they generate.
- **No issues found.**
:::

:::success
### [QZ2-307] Non-malleability of Unified Addresses and Unified Viewing Keys §
- **Description:** The use of the F4Jumble algorithm in the address hardening process of the encoding adds enough redundancy to the UA encoding that no malleability is possible of the underlying address and / or prioritization
    - A consumer of a UA is not able to malleate the UA to add an address under its control. This would break malleability, which is ensured by F4Jumble.
    - As discussed in the ZIP-316 and in the [Gentry and Ramzam paper](https://iacr.org/archive/asiacrypt2004/33290031/33290031.pdf), a four-round Feistel network is enough for a random permution. In this paper they argue that the construction is secure under the strong security property, preventing even partial collisions.
- **No issues found.**
:::

---------------------------------------------------------------------------------------------
## Orchard Actions & Protocol [ID QZ*-4**]



:::info
### [QZ1-401] Lack of Orchard Shielded Protocol Proof of Security

- **Description:** Throughout the specification there are very good intuitive notes and arguments for how the different components of the system achieve security. However, there is no overarching proof of security, or high-level sketch of the proof, as was done for the Sapling protocol [GabizonHopwood](https://github.com/zcash/sapling-security-analysis). This implies that not having a proof means that one may miss important security issues for requirements or security properties such as 
    - non-malleability, 
    - unlinkability / indistinguishability, 
    - balance, 
    - spendability / ownership 
    
    This is true especially given some of the changes from the Sapling protocol (non-malleability of transaction hash, unified addresses, etc.)

- **Recommendation:** Provide an overarching proof of security, or at least a sketch at the end of the Orchard protocol description. Another example for such a proof is in Appendix B of [Decentralized Anonymous Micropayments](https://eprint.iacr.org/2016/1033.pdf).
:::

:::info
### [QZ1-402] Nullifier Generation Clarification Needed in the specification [§4.16](https://zips.z.cash/protocol/nu5.pdf#commitmentsandnullifiers)

- **Description:** The generation of the nullifier in the Orchard protocol is one of the most sensitive components, since there are many potential attacks that could be implemented if any part of the derivation is not secured robustly. The nullifier creates a linked chain of the asset's life, while generating the necessary sender-controlled randomness to ensure the nullifier's uniqueness:
    - As defined in [ZIP 224](https://zips.z.cash/zip-0224#nullifiers), nullifiers are generated using Poseidon hash and as a combination of the nullifier key, $\mathsf{nk}$, $\rho$ , $\psi$ and the note commitment itself, $\mathsf{cm}$:
$$
\mathsf{nf} = [F_{nk}(\rho) + \psi (\mod p)]\mathcal{G} + \mathsf{cm}
$$
    - In Section §4.17.4 it appears that $\mathsf{\rho_{new}} = \mathsf{nf_{old}}$, for which a new random element must be generated to make the nullifier unique, i.e.: $\mathsf{\psi_{new}}$
    - As explained in [§3.5 of the Orchard book](https://zcash.github.io/orchard/design/nullifiers.html), the nullifier derivation process must ensure several properties:
        - Balance of funds
        - Note privacy (in and out of band)
        - Spend unlinkability
        - Faerie-gold attack resistance

- **Recommendation**: follow similar arguments in the spec to those outlined in [§3.5 of the Orchard book](https://zcash.github.io/orchard/design/nullifiers.html) for arguing why the nullifier generation yields a secure protocol.
:::

:::success
### [QZ1-403] Action Statement Integrity [§4.17.4](https://zips.z.cash/protocol/nu5.pdf#actions)

- **Description**: The Action statement for the Orchard protocol differs from Sapling's Spend & Output in various ways, as outlined in [ZIP 244](https://zips.z.cash/zip-0224). One of these is that the two proofs are combined into a single statement, hence proving the integrity of the spend and output notes, commitments, derived values. The two subcircuits are mostly maintained, with the spend checking the note commitment, merkle path and nullifier integrity and the output checking the new note commitment integrity. We checked the different integrity checks 
    1. Both spend and output note commitments are generated correctly, fixing the owner, value and nullifier randomness
    2. The commitment belongs to the set (tree) of existing note commitments
    3. The spend note produces a valid nullifier - its uniquess is critical for preventing double-spend attacks. The validator nodes MUST verify that the nullifier has not yet been published to the blockchain.
    4. The re-randomized signature public key, $\mathsf{rk}$ was generated properly from the spend note's owner's unique signature public key, $\mathsf{ak}$. This ensures that the transaction signature authorizing the transfer is indeed done by the owner of the note being spent.
    5. The "diversifier address integrity" performs a critical role: ensuring that the $\mathsf{ak}, \mathsf{rk}, \mathsf{pk_d}$ are all related to the same spending key, $\mathsf{sk}$. If this check was not in place, person A could spend notes that are not owned by A.
    6. The values of all the notes have been committed correctly, using the same values as in the note commitments above. The value commitment is done by taking $\mathsf{v_{old}} - \mathsf{v_{new}}$ and committing to this difference. The validator nodes MUST verify that the addition of all value commitments is equivalent to a commitment to zero. This is done by verifying a homomorphic signature.
- **No issue found.**
- **Recommentation:** For each of the non-normative notes in Section §4.17.4, there should be a sketch or proof of security alluding to each of these. Furthermore, it is important to clarify that the integrity of the statement is not purely contained in the proof being verified, but also in further verifications (either by all the network or by the receiver of the transfer). Some examples include: nullifier list check, SpendSignature and BalanceSig verification, etc. These are critical and deserve their mention and security sketch.
:::


:::success
### [QZ2-404] V2 History tree integration [PR#5227](https://github.com/zcash/zcash/pull/5227)

- **Description**: Added support of "Orchard" commitment tree.
    - Extracted the tree data structure into the zcash_history crate.
    - Proper and extensive testing.
- **No issue found.**
:::


:::success
### [QZ2-405] ZIP 216 activation in zcash/zcash [PR#5213](https://github.com/zcash/zcash/pull/5213/files)

- **Description**: Upgraded dependencies `zcash_primitives` and `zcash_proofs` and added the `nu5Active` flag to the verification context.
    - Properly documented.
    - Updated FFI
- **No issues found.**
:::


:::success
### [QZ2-406] Trigger Orchard consensus rules verification in zcash/zcash [PR#5232](https://github.com/zcash/zcash/pull/5232/files)

- **Description**: added call to `orchardBundle.CheckBundleSpecificConsensusRules()` in the client.
    - Properly documented.
    - Updated FFI
- **No issues found.**
:::

:::info
### [QZ2-407] Rust bridge / FFI mechanism in zcash/zcash [orchard.h](https://github.com/zcash/zcash/blob/master/src/rust/include/rust/orchard.h), [orchard_ffi.rs](https://github.com/zcash/zcash/blob/master/src/rust/src/orchard_ffi.rs) and [orchard.h](https://github.com/zcash/zcash/blob/master/src/rust/include/rust/orchard.h).

- **Description**: Implemented bridge between C and Rust.
    - Properly implemented
    - Properly documented.
- **No issues found.**
- **Recommendation:** add a test to make sure that the C data structures are fully interoperable with the Rust data structures.
:::


:::success
### [QZ2-408] Consensus rules for transaction V5 [PR#5225](https://github.com/zcash/zcash/pull/5225)

- **Description**: added consensus rules to the zcash client.
    - Compatible with the [specification](https://zips.z.cash/protocol/nu5.pdf#txnencodingandconsensus).
    - Properly documented.
- **No issues found.**
:::


:::info
### [QZ2-409] RNG and source of entropy on virtual machine (VM) in the *Cloud*

- **Description**: Privacy of transaction and zero-knowledge property may not be ensured if the prover runs on an improperly-configured VM.
    - Random numbers are required everywhere to ensure zero-knowledge property / privacy.
    - On Intel Machines (and some AMD as well), a call to *getrandom* is remapped onto the *RDRAND* CPU instruction. But when launched on a VM, this remapping is handled by the hypervisor... only if it has been properly configured.
    - The internal RNG uses a syscall that reads `/dev/urandom` on Unix systems.
    - [This link](https://www.exoscale.com/syslog/random-numbers-generation-in-virtual-machines/#RDRAND) states that 
    > **/dev/urandom**: lesser — but still high — quality of randomness, generated by an intermediate Cryptographically (Secure) Random Number Generator (CRNG); it will **block until it is properly seeded from the entropy pool** (at boot time) but **not afterwards** (albeit with decreasing randomness quality if you read too many data, too fast, before it gets re-seeded)
    - In cloud environments, a hacker owning a machine on the same host could empty the entropy available in `/dev/urandom` asking for a huge amount of random data. This will decrease the randomness quality of subsequent calls, and may lower the security.
- **Recommendation**: We don't think these issues could be fixed since the running environment is out-of-scope, and cannot be controlled. So trust is the key here.
:::

:::warning
### [QZ2-410] Randomness reuse in restored virtual machines

- **Description**: Privacy of transaction and zero-knowledge property may not be maintained when transactions are sent from restarted VMs

    - As pointed out by [QZ2-409](#[QZ2-409]-RNG-and-source-of-entropy) the internal RNG uses a syscall that reads `/dev/urandom` on Unix systems, which (in the absence of fresh entropy) behaves deterministically.
    - Thus, if VM state is saved, a random value is drawn, and then the VM is reset to the save state and randomness is re-generated, and a random number is drawn again (quickly enough), then the two values may be identical with high probability. (See. e.g., the [When Good Randomness Goes Bad](https://rist.tech.cornell.edu/papers/sslhedge.pdf) paper.)
    - Such randomness reuse may break the zero-knowledge properties of the proving system, or other privacy properties of the Orchard protocol.
    - It affects commitments, hashes and other primitives, except for the signatures that are deterministic.
- **Recommendation**: Find and implement best practices.
:::


## Circuits [ID QZ*-5**]

This section includes the issues reported on the Action circuit, which can be found at [this link](https://github.com/zcash/orchard/blob/main/src/circuit.rs).

![](https://i.imgur.com/XCV5GAx.jpg)


This diagram is a visual representation of the Action circuit and the relations between the different variables used. It includes the type of variables, the functions invoked that map each of the variables, as well as a color code to show what is private / public. Here is a more concrete legend:
- Variable types
    - parallelograms represent base field elements
    - triangles represent byte strings
    - rectangles with round corners represent Pallas elliptic curve group elements
    - the marquise shapes represent values in a given range
- Color coding
    - red represents private elements, not shareable with anyone else
    - green represents public elements, shareable with all the network as part of the transaction
    - yellow represents elements that are known by the receiver of the transfer
    - orange represents elements that can be shared with external parties but that imply a loss in privacy
    - blue represents intermediate computations that must be verified as constraints in the circuit

**EDITThere were last minute changes to the types of the diagram variables to adapt to the [finding reported by the ECC team](https://github.com/zcash/zips/issues/560) of ill-typed `MerkleCRH` 

:::info
### [QZ2-501] Naming style and convention of gate definition and usage

- **Description:** There is a kind of "disconnection" between gate definition, and usage. There is a calling convention of enabling a selector on the right row, and low-level copies of arguments into anonymous columns and offsets (e.g., advices[4], uint offset vs Rotation type). 
- **Recommendation:** The suggestion is to give names to every entity such as columns, cells, and offsets, using named functions with named arguments. Maybe common getter functions to access cells. Generally try to unify the "config" items shared between definitions and invocations in the type system (like most other concepts do enjoy a good level of type safety). 
    - Example: v_net logic (still simple, but does not belong in the main synthesize function as an anonymous block); note commit (very complex including very indirect contracts on value range checks); short fixed-base multiplication (very indirect with magnitude.y = y_qr at offset+1).
:::

:::info
### [QZ2-502] Naming style for regions and columns
- **Description:** In the circuit code dealing with regions and columns, there are a few naming style issues:
    - `SingleChipLayouter.columns` is column_sizes and `regions` is region_starts, so the naming does not convey the real meaning
    - Some of the functions named `assign_region` mean `allocate_region`, while others mean `assign_into_region`.
- **Recommendations:** 
    - Wrap indexing into a function (`layouter.regions[region_index] + offset`), which will also help to ensure type safety
    - Give names of locations in regions instead of e.g., `(config.advices[0], offset 0)`
:::

:::info
### [QZ2-503] Unfixed offset for the instance column count
- **Description:** There are [exactly 9 instance inputs/outputs](https://github.com/zcash/orchard/blob/b4a82211cee82ceb02d2e0e99b7566a967804a6c/src/circuit.rs#L68) to the circuit, yet this is not fixed anywhere.
- **Recommendation:** In the same way that the advice column has a fixed constant, make `9` a named constant.
:::

:::info
### [QZ2-504] Minor inefficiency in cloning patterns 
- **Description:** In several places (i.e.: `ecc_chip`, `sinsemilla_config`), the `clone()` function is used to copy large structures, which can cause minor inefficiency.
- **Recommendation:** Use references where possible
:::

:::info
### [QZ2-505] note_commit_config region assignment
- **Description:** the `layouter.namespace` of the note commitment configuration is the same for the *old* and *new* notes, prone to mistake or confusing logs
- **Recommendation:** Fix it.
:::

:::success
### [QZ2-506] No unintended consequences of `v_old` and unconstrained `anchor`
- **Description:** With `v_old=0`, 
    - the old note did not go through “new note integrity”
    - “Old note integrity” does check again all the checks of “new note integrity” (`gd` and `pk_d` are points, the note commitment is calculated, value is 64 bits).
- **No issues found.**
:::



:::success
### [QZ2-507] v_net and anchor logic check
- **Description:**
    - [x] The connection between `v_old`, `v_new`, `magnitude`, `sign`, `anchor` is verified and the same variables are used in all computations.
    - [x] `magnitude` and `sign` range check. Magnitude 66 bits in decompose. Then in `q_mul_fixed_short`, `last_window_check` 0 or 1 (`advices[5]`), and sign +1/-1 (window `advices[4]`).
    - [x] `v_old` and `v_new` are in range 64 bits (`note_commit.assign_region`).
        - [x] Part of the `q_canon_1` gate (`advices[2]`).
        - [x] Combination of `d_2`, `z1_d`, `e_0`.
        - [x] `d_2` (`advices[3]`)in range 8 bits, `witness_short_check`.
        - [x] `z1_d = d_3` (`advices[4]`) in range 50 bits.
        - [x] `e_0` (`advices[5]`) in range 6 bits, `witness_short_check`.
    - [x] `v_old = 0` or `enable_spends = 1`
    - [x] `v_new = 0` or `enable_outputs = 1`; `value=1` to enable, anything else to disable.
    - [x] v_old - v_new = magnitude * sign.
    - [x] anchor matches public input if v_old != 0.
    - [x] correct connectivity of circuit variables to chip.
- **No issues found.**
- **Recommendation:** implement as a sequence of simple add/mul gates (4 gates for q_orchard and 1 for the addition in nullifiers), instead of using a custom gate.
:::

:::success
### [QZ2-508] Value commitment integrity check
- **Description:**
    - [x] negation of point by negation of `y`.
    - [x] Connection between `magnitude_mul`, and `offset+1`, `y_qr`, `sign`, and output point `y_p`.
    - [x] signed point (already seen for `magnitude_sign` range check).
    - [x] Commit and blind with fixed-base mul short ``(magnitude,sign)``, fixed-base mul full non-canonical `rcv`, complete addition, `([v_net] ValueCommitV + [rcv] ValueCommitR)`.
    - [x] Connection to public inputs.
- **No issues found.**
:::


:::success
### [QZ2-509] Note commitment check
- **Description:** We verified that the following properties hold
    - Old note commitment
        - [x] `cm`, `g_d`, `ak` are points.
        - [x] `psi_old` connection between commitment and nullifier.
        - [x] Connection of `cm_old` between commit, nullifier and leaf of Merkle path.
- **No issues found.**

:::


:::success
### [QZ2-510] Nullifier old check
- **Description:** We verified that the that the nullifier is properly generated
    1. The `poseidon_hash(nk, rho_old)` is computed correctly
        a. Checked the connection of `nk` across the circuit (advices[0] offset 1 in `q_commit_ivk`).
    2. The modular addition is done correctly in the circuit.
    3. The EC exponentiation of `NullifierK` is computed correctly.
    4. Add `cm_old` with complete addition.
- **No issues found.**

:::


:::success
### [QZ2-511] Elliptic curve operations as constraints
- **Description:** We verified that the following properties hold
  - [x] ECAdd region assigned + operations
  - [x] ECIncompAdd region assigned + operations
  - [x] ECMul[Fixed] region assigned + operations
  - [X] Witnesses are Pallas Points 
- **No issues found**

:::


:::success
### [QZ2-512] Sinsemilla chip and gadget check
- **Description:** We verified the following
  - [x] Regions assignment coherency
  - [x] Parameters constraints
  - [x] SubChips:
      - [x] Merkle Chips
      - [x] CommitIvk
      - [x] NoteCommit
- **No issues found**

:::

:::success
### [QZ2-513] New note integrity check
- **Description:** We verified that the following properties hold
    - [x] `psi_new` and `rcm_new`, no constraints necessary.
    - [x] public input connections. (Only X coordinate).
    - [x] `g_d_new` and `pk_d_new` are points.
    - [x] old nullifier connection.
    - [x] value `v_new` connection.
    - [x] commitment with full-width blinding factor.
- **No issues found.**

:::

:::success
### [QZ2-514] Diversified address integrity check
- **Description:** We verified that the following properties hold
    - [x] Connection to `gd_old` in note commitment.
    - [x] `gd_old` cannot be the zero point
        -  it is declared as a non-zero point g_d_old: `Option<NonIdentityPallasPoint>`
        -  the default value is the generator of the curve, (which is obviously non zero)
        -  and when imported from a byte buffer, it's checked again against the zero point
    - [x] Connection to `ak` in spend authority.
    - [x] Connection to `nk` in nullifier.
    - [x] Derivation of `ivk` from `nk` and `ak` (X coordinate).
    - [x] `ivk` cannot be a congruence (base field < scalar field order).
    - [x] Derivation of `pk_d_old` from `gd_old` and `ivk`. Variable mul with scalar (Pallas base field).
    - [x] `pk_d_old` is a point.
    - [x] Connection of `pk_d_old` to note commitment (X, Y coordinates).
- **No issues found.**
- ***EDIT 29/09/2021:** a warning issue was found by the ECC team after the report was shared. In fact, the circuit did not originally check that `gd_old` is not the zero point, contrary to our report. For more information, see [this PR](https://github.com/zcash/orchard/pull/209).*
:::

:::success
### [QZ2-515] Spend authority check
- **Description:** We verified that the following properties hold
    - [x] Exponentiation with full-width scalar (no constraint on `alpha` needed).
    - [x] Add `ak` with complete addition.
    - [x] `ak` is a point.
    - [x] Connection of `ak` to `ivk` (X coordinate).
    - [x] Public input connection of `rk`. (X and Y coordinates).
- **No issues found.**
:::

:::info
### [QZ2-516] Typo in circuit comment
- **Description:** wrong comment in ecc/chip.rs constrain_equal - “Constraint x-coordinate"
:::
