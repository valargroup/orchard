use alloc::vec::Vec;
use core::fmt;

use ff::PrimeField;
use pasta_curves::pallas;

use crate::{
    keys::{FullViewingKey, SpendValidatingKey},
    note::{ExtractedNoteCommitment, Note, NoteVersion, Nullifier, RandomSeed, Rho},
    note_encryption::{IronwoodDomain, IronwoodNoteEncryption, OrchardDomain, OrchardNoteEncryption},
    value::{NoteValue, ValueCommitTrapdoor, ValueCommitment},
    Address,
};
use zcash_note_encryption::Domain;

/// Byte-level recompute primitives for the six derived Orchard PCZT fields.
///
/// These free functions are the single source of truth for deriving `cv_net`, `nullifier`,
/// `rk`, `cmx`, `ephemeral_key`, and `enc_ciphertext` from a note's component fields. They
/// are used in two places:
///
/// - The `Action`/`Spend`/`Output` `verify_*` methods (which compare the recomputed value
///   against a stored field), and
/// - The PCZT recompute-and-fill path in `pczt` (which calls these directly on the wire
///   bytes to populate an omitted field *before* the protocol structs are parsed, so that
///   every downstream consumer sees fully-populated actions).
///
/// Keeping the derivation in one place ensures the value a sender omits is byte-identical to
/// the value a verifier would have checked.
pub mod recompute {
    use super::*;

    /// Reconstructs a note from its raw component byte fields.
    fn note_from_bytes(
        recipient: &[u8; 43],
        value: u64,
        rho: Rho,
        rseed: &[u8; 32],
        note_version: NoteVersion,
    ) -> Result<Note, VerifyError> {
        let recipient = Address::from_raw_address_bytes(recipient)
            .into_option()
            .ok_or(VerifyError::MissingRecipient)?;
        let rseed = RandomSeed::from_bytes(*rseed, &rho)
            .into_option()
            .ok_or(VerifyError::MissingRandomSeed)?;
        Note::from_parts(
            recipient,
            NoteValue::from_raw(value),
            rho,
            rseed,
            note_version,
        )
        .into_option()
        .ok_or(VerifyError::InvalidSpendNote)
    }

    /// Recomputes `cv_net` from the spend and output values and the value commitment
    /// trapdoor, returning its byte encoding.
    pub fn cv_net(
        spend_value: u64,
        output_value: u64,
        rcv: &[u8; 32],
    ) -> Result<[u8; 32], VerifyError> {
        let rcv = ValueCommitTrapdoor::from_bytes(*rcv)
            .into_option()
            .ok_or(VerifyError::MissingValueCommitTrapdoor)?;
        let value = NoteValue::from_raw(spend_value) - NoteValue::from_raw(output_value);
        Ok(ValueCommitment::derive(value, rcv).to_bytes())
    }

    /// Recomputes a spent note's `nullifier` from its component fields and full viewing key,
    /// returning its byte encoding.
    pub fn nullifier(
        recipient: &[u8; 43],
        value: u64,
        rho: &[u8; 32],
        rseed: &[u8; 32],
        fvk: &[u8; 96],
        note_version: NoteVersion,
    ) -> Result<[u8; 32], VerifyError> {
        let rho = Rho::from_bytes(rho)
            .into_option()
            .ok_or(VerifyError::MissingRho)?;
        let note = note_from_bytes(recipient, value, rho, rseed, note_version)?;
        let fvk = FullViewingKey::from_bytes(fvk).ok_or(VerifyError::MissingFullViewingKey)?;
        // The nullifier only constrains `nk` within the FVK; we additionally confirm the
        // FVK owns the note, matching `Spend::verify_nullifier`.
        fvk.scope_for_address(&note.recipient())
            .ok_or(VerifyError::WrongFvkForNote)?;
        Ok(note.nullifier(&fvk).to_bytes())
    }

    /// Recomputes a spend's randomized verification key `rk` from its full viewing key and
    /// spend authorization randomizer, returning its byte encoding.
    pub fn rk(fvk: &[u8; 96], alpha: &[u8; 32]) -> Result<[u8; 32], VerifyError> {
        let fvk = FullViewingKey::from_bytes(fvk).ok_or(VerifyError::MissingFullViewingKey)?;
        let alpha = pallas::Scalar::from_repr(*alpha)
            .into_option()
            .ok_or(VerifyError::MissingSpendAuthRandomizer)?;
        let ak = SpendValidatingKey::from(fvk);
        Ok((&ak.randomize(&alpha)).into())
    }

    /// Reconstructs an output note from its component fields. The output note's `rho` is
    /// derived from the spent note's nullifier, so `spend_nullifier` must be the already
    /// recomputed (or supplied) nullifier of the same action.
    fn output_note(
        recipient: &[u8; 43],
        value: u64,
        spend_nullifier: &[u8; 32],
        rseed: &[u8; 32],
        note_version: NoteVersion,
    ) -> Result<Note, VerifyError> {
        let nf = Nullifier::from_bytes(spend_nullifier)
            .into_option()
            .ok_or(VerifyError::InvalidNullifier)?;
        let rho = Rho::from_nf_old(nf);
        note_from_bytes(recipient, value, rho, rseed, note_version).map_err(|_| {
            // Distinguish output-note reconstruction failure from spend-note failure.
            VerifyError::InvalidOutputNote
        })
    }

    /// Recomputes an output's note commitment `cmx` from its component fields, returning its
    /// byte encoding.
    pub fn cmx(
        recipient: &[u8; 43],
        value: u64,
        spend_nullifier: &[u8; 32],
        rseed: &[u8; 32],
        note_version: NoteVersion,
    ) -> Result<[u8; 32], VerifyError> {
        let note = output_note(recipient, value, spend_nullifier, rseed, note_version)?;
        Ok(ExtractedNoteCommitment::from(note.commitment()).to_bytes())
    }

    /// Recomputes an output's `ephemeral_key` and `enc_ciphertext` from its component
    /// fields, returning `(ephemeral_key, enc_ciphertext)`.
    ///
    /// The note plaintext version selects the note encryption domain (V2 → Orchard, V3 →
    /// Ironwood), and the memo is the empty memo `[0u8; 512]`, matching the encoding used by
    /// the restricted builder's real migration output. Encryption uses an `ovk` of `None`;
    /// this affects only `out_ciphertext`, which is never recomputed (it is RNG-derived and
    /// kept required).
    pub fn ephemeral_key_and_enc_ciphertext(
        recipient: &[u8; 43],
        value: u64,
        spend_nullifier: &[u8; 32],
        rseed: &[u8; 32],
        note_version: NoteVersion,
    ) -> Result<([u8; 32], Vec<u8>), VerifyError> {
        let note = output_note(recipient, value, spend_nullifier, rseed, note_version)?;
        // `ephemeral_key` (the epk) is memo-independent and is reproduced exactly here.
        //
        // WARNING: the recomputed `enc_ciphertext` is built under a [0u8; 512] memo and is only
        // validated against [0u8; 512]-memo fixtures. It does NOT reproduce a real migration
        // output, which is built with `MemoBytes::empty()` (ZIP 302, lead byte 0xF6); for
        // Ironwood (V3) the exact plaintext/memo encoding has not been pinned down. Consequently
        // `enc_ciphertext` must NOT be omitted on the inbound (wallet->device) path: the wallet
        // keeps it and the device refills it from the wallet's clone via the combiner on the
        // outbound path. Pinning down the Ironwood V3 enc_ciphertext encoding is a prerequisite
        // to dropping `enc_ciphertext` inbound.
        let out = match note_version {
            NoteVersion::V2 => {
                let encryptor = OrchardNoteEncryption::new(None, note, [0u8; 512]);
                (
                    OrchardDomain::epk_bytes(encryptor.epk()).0,
                    encryptor.encrypt_note_plaintext().to_vec(),
                )
            }
            NoteVersion::V3 => {
                let encryptor = IronwoodNoteEncryption::new(None, note, [0u8; 512]);
                (
                    IronwoodDomain::epk_bytes(encryptor.epk()).0,
                    encryptor.encrypt_note_plaintext().to_vec(),
                )
            }
        };
        Ok(out)
    }
}

impl super::Bundle {
    /// If this bundle disables cross-address transfers, verifies that every action's
    /// output is addressed to the same expanded receiver (`(g_d, pk_d)`) as its spent
    /// note. This is a no-op for bundles that permit cross-address transfers.
    ///
    /// When the restriction applies, it requires `spend.recipient` and `output.recipient`
    /// to be set on every action. Signers should always call this before signing. The
    /// equivalent structural checks are also performed by [`Bundle::finalize_io`] and
    /// `Bundle::create_proof`.
    ///
    /// The post-NU6.3 circuit supports enforcing the restriction; older circuit versions
    /// do not. The prover and verifier APIs reject restricted bundles for those keys.
    /// (That is not a security restriction; for security, the consensus verifier must use
    /// the correct key for the epoch and pool.)
    ///
    /// [`Bundle::finalize_io`]: super::Bundle::finalize_io
    pub fn verify_cross_address_restriction(&self) -> Result<(), VerifyError> {
        if !self.flags.cross_address_enabled() {
            for action in &self.actions {
                let spend_recipient = action
                    .spend
                    .recipient
                    .ok_or(VerifyError::MissingRecipient)?;
                let output_recipient = action
                    .output
                    .recipient
                    .ok_or(VerifyError::MissingRecipient)?;

                if !spend_recipient.same_expanded_receiver(&output_recipient) {
                    return Err(VerifyError::DisallowedCrossAddressTransfer);
                }
            }
        }

        Ok(())
    }
}

impl super::Action {
    /// Verifies that the `cv_net` field is consistent with the note fields.
    ///
    /// Requires that the following optional fields are set:
    /// - `spend.value`
    /// - `output.value`
    /// - `rcv`
    pub fn verify_cv_net(&self) -> Result<(), VerifyError> {
        let spend_value = self.spend().value.ok_or(VerifyError::MissingValue)?;
        let output_value = self.output().value.ok_or(VerifyError::MissingValue)?;
        let rcv = self
            .rcv
            .clone()
            .ok_or(VerifyError::MissingValueCommitTrapdoor)?;

        let cv_net = ValueCommitment::derive(spend_value - output_value, rcv);
        if cv_net.to_bytes() == self.cv_net.to_bytes() {
            Ok(())
        } else {
            Err(VerifyError::InvalidValueCommitment)
        }
    }
}

impl super::Spend {
    /// Returns the [`FullViewingKey`] to use when validating this note.
    ///
    /// Handles dummy notes when the `value` field is set.
    fn fvk_for_validation<'a>(
        &'a self,
        expected_fvk: Option<&'a FullViewingKey>,
    ) -> Result<&'a FullViewingKey, VerifyError> {
        match (expected_fvk, self.fvk.as_ref(), self.value.as_ref()) {
            (Some(expected_fvk), Some(fvk), _) if fvk == expected_fvk => Ok(fvk),
            // `expected_fvk` is ignored if the spent note is a dummy note.
            (Some(_), Some(fvk), Some(value)) if value.inner() == 0 => Ok(fvk),
            (Some(_), Some(_), _) => Err(VerifyError::MismatchedFullViewingKey),
            (Some(expected_fvk), None, _) => Ok(expected_fvk),
            (None, Some(fvk), _) => Ok(fvk),
            (None, None, _) => Err(VerifyError::MissingFullViewingKey),
        }
    }

    /// Verifies that the `nullifier` field is consistent with the note fields.
    ///
    /// Requires that the following optional fields are set:
    /// - `recipient`
    /// - `value`
    /// - `rho`
    /// - `rseed`
    ///
    /// In addition, at least one of the `fvk` field or `expected_fvk` must be provided.
    ///
    /// The provided [`FullViewingKey`] is ignored if the spent note is a dummy note.
    /// Otherwise, it will be checked against the `fvk` field (if both are set).
    pub fn verify_nullifier(
        &self,
        expected_fvk: Option<&FullViewingKey>,
    ) -> Result<(), VerifyError> {
        let fvk = self.fvk_for_validation(expected_fvk)?;

        let note = Note::from_parts(
            self.recipient.ok_or(VerifyError::MissingRecipient)?,
            self.value.ok_or(VerifyError::MissingValue)?,
            self.rho.ok_or(VerifyError::MissingRho)?,
            self.rseed.ok_or(VerifyError::MissingRandomSeed)?,
            self.note_version,
        )
        .into_option()
        .ok_or(VerifyError::InvalidSpendNote)?;

        // We need both the note and the FVK to verify the nullifier; we have everything
        // needed to also verify that the correct FVK was provided (the nullifier check
        // itself only constrains `nk` within the FVK).
        fvk.scope_for_address(&note.recipient())
            .ok_or(VerifyError::WrongFvkForNote)?;

        if note.nullifier(fvk) == self.nullifier {
            Ok(())
        } else {
            Err(VerifyError::InvalidNullifier)
        }
    }

    /// Verifies that the `rk` field is consistent with the given FVK.
    ///
    /// Requires that the following optional fields are set:
    /// - `alpha`
    ///
    /// The provided [`FullViewingKey`] is ignored if the spent note is a dummy note
    /// (which can only be determined if the `value` field is set). Otherwise, it will be
    /// checked against the `fvk` field (if set).
    pub fn verify_rk(&self, expected_fvk: Option<&FullViewingKey>) -> Result<(), VerifyError> {
        let fvk = self.fvk_for_validation(expected_fvk)?;

        let ak = SpendValidatingKey::from(fvk.clone());

        let alpha = self
            .alpha
            .as_ref()
            .ok_or(VerifyError::MissingSpendAuthRandomizer)?;

        if ak.randomize(alpha) == self.rk {
            Ok(())
        } else {
            Err(VerifyError::InvalidRandomizedVerificationKey)
        }
    }
}

impl super::Output {
    /// Verifies that the `cmx` field is consistent with the note fields.
    ///
    /// Requires that the following optional fields are set:
    /// - `recipient`
    /// - `value`
    /// - `rseed`
    ///
    /// `spend` must be the Spend from the same Orchard action.
    pub fn verify_note_commitment(&self, spend: &super::Spend) -> Result<(), VerifyError> {
        let note = Note::from_parts(
            self.recipient.ok_or(VerifyError::MissingRecipient)?,
            self.value.ok_or(VerifyError::MissingValue)?,
            Rho::from_nf_old(spend.nullifier),
            self.rseed.ok_or(VerifyError::MissingRandomSeed)?,
            self.note_version,
        )
        .into_option()
        .ok_or(VerifyError::InvalidOutputNote)?;

        if ExtractedNoteCommitment::from(note.commitment()) == self.cmx {
            Ok(())
        } else {
            Err(VerifyError::InvalidExtractedNoteCommitment)
        }
    }
}

/// Errors that can occur while verifying a PCZT bundle.
#[derive(Debug)]
#[non_exhaustive]
pub enum VerifyError {
    /// An action's output is addressed differently than its spent note, but the bundle's pool
    /// restriction disables cross-address transfers.
    DisallowedCrossAddressTransfer,
    /// The output note's components do not produce the expected `cmx`.
    InvalidExtractedNoteCommitment,
    /// The spent note's components do not produce the expected `nullifier`.
    InvalidNullifier,
    /// The output note's components do not produce a valid note commitment.
    InvalidOutputNote,
    /// The Spend's FVK and `alpha` do not produce the expected `rk`.
    InvalidRandomizedVerificationKey,
    /// The spent note's components do not produce a valid note commitment.
    InvalidSpendNote,
    /// The action's `cv_net` does not match the provided note values and `rcv`.
    InvalidValueCommitment,
    /// The spend or output's `fvk` field does not match the provided FVK.
    MismatchedFullViewingKey,
    /// Dummy notes must have their `fvk` field set in order to be verified.
    MissingFullViewingKey,
    /// `nullifier` verification requires `rseed` to be set.
    MissingRandomSeed,
    /// Verification requires `recipient` to be set.
    MissingRecipient,
    /// `nullifier` verification requires `rho` to be set.
    MissingRho,
    /// `rk` verification requires `alpha` to be set.
    MissingSpendAuthRandomizer,
    /// Verification requires all `value` fields to be set.
    MissingValue,
    /// `cv_net` verification requires `rcv` to be set.
    MissingValueCommitTrapdoor,
    /// The provided `fvk` does not own the spent note.
    WrongFvkForNote,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::DisallowedCrossAddressTransfer => write!(
                f,
                "an action outputs to a different expanded receiver than it spends from, but the \
                 bundle's pool restriction disables cross-address transfers"
            ),
            VerifyError::InvalidExtractedNoteCommitment => {
                write!(f, "output note doesn't match `cmx`")
            }
            VerifyError::InvalidNullifier => write!(f, "spent note doesn't match `nullifier`"),
            VerifyError::InvalidOutputNote => write!(f, "invalid output note"),
            VerifyError::InvalidRandomizedVerificationKey => {
                write!(f, "spend's `fvk` and `alpha` do not match `rk`")
            }
            VerifyError::InvalidSpendNote => write!(f, "invalid spent note"),
            VerifyError::InvalidValueCommitment => {
                write!(f, "`cv_net` doesn't match the note values and `rcv`")
            }
            VerifyError::MismatchedFullViewingKey => {
                write!(f, "Provided full viewing key doesn't match the `fvk` field")
            }
            VerifyError::MissingFullViewingKey => write!(f, "`fvk` missing for dummy note"),
            VerifyError::MissingRandomSeed => {
                write!(f, "`rseed` missing for `nullifier` verification")
            }
            VerifyError::MissingRecipient => write!(f, "`recipient` missing for verification"),
            VerifyError::MissingRho => write!(f, "`rho` missing for `nullifier` verification"),
            VerifyError::MissingSpendAuthRandomizer => {
                write!(f, "`alpha` missing for `rk` verification")
            }
            VerifyError::MissingValue => write!(f, "`value` missing"),
            VerifyError::MissingValueCommitTrapdoor => {
                write!(f, "`rcv` missing for `cv_net` verification")
            }
            VerifyError::WrongFvkForNote => write!(f, "`fvk` does not own the action's spent note"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VerifyError {}
