//! Struct for managing extra required account configs, ie. defining accounts
//! required for your interface program, which can be  `AccountMeta`s - which
//! have fixed addresses - or PDAs - which have addresses derived from a
//! collection of seeds

use {
    crate::{error::AccountResolutionError, seeds::Seed},
    bytemuck::{Pod, Zeroable},
    solana_program::{
        account_info::AccountInfo, instruction::AccountMeta, program_error::ProgramError,
        pubkey::Pubkey,
    },
    spl_type_length_value::pod::PodBool,
};

/// Resolve a program-derived address (PDA) from the instruction data
/// and the accounts that have already been resolved
fn resolve_pda(
    seeds: &[Seed],
    accounts: &[AccountMeta],
    instruction_data: &[u8],
    program_id: &Pubkey,
) -> Result<Pubkey, ProgramError> {
    let mut pda_seeds: Vec<&[u8]> = vec![];
    for config in seeds {
        match config {
            Seed::Uninitialized => (),
            Seed::Literal { bytes } => pda_seeds.push(bytes),
            Seed::InstructionData { index, length } => {
                let arg_start = *index as usize;
                let arg_end = arg_start + *length as usize;
                pda_seeds.push(&instruction_data[arg_start..arg_end]);
            }
            Seed::AccountKey { index } => {
                let account_index = *index as usize;
                let account_meta = accounts
                    .get(account_index)
                    .ok_or::<ProgramError>(AccountResolutionError::AccountNotFound.into())?;
                pda_seeds.push(account_meta.pubkey.as_ref());
            }
        }
    }
    Ok(Pubkey::find_program_address(&pda_seeds, program_id).0)
}

/// `Pod` type for defining a required account in a validation account.
///
/// This can either be a standard `AccountMeta` or a PDA.
/// Can be used in TLV-encoded data.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ExtraAccountMeta {
    /// Discriminator to tell whether this represents a standard
    /// `AccountMeta` or a PDA
    pub discriminator: u8,
    /// This `address_config` field can either be the pubkey of the account
    /// or the seeds used to derive the pubkey from provided inputs
    pub address_config: [u8; 32],
    /// Whether the account should sign
    pub is_signer: PodBool,
    /// Whether the account should be writable
    pub is_writable: PodBool,
}
impl ExtraAccountMeta {
    /// Create a `ExtraAccountMeta` from a public key,
    /// thus representing a standard `AccountMeta`
    pub fn new_with_pubkey(
        pubkey: &Pubkey,
        is_signer: bool,
        is_writable: bool,
    ) -> Result<Self, ProgramError> {
        Ok(Self {
            discriminator: 0,
            address_config: pubkey.to_bytes(),
            is_signer: is_signer.into(),
            is_writable: is_writable.into(),
        })
    }

    /// Create a `ExtraAccountMeta` from a list of seed configurations,
    /// thus representing a PDA
    pub fn new_with_seeds(
        seeds: &[Seed],
        is_signer: bool,
        is_writable: bool,
    ) -> Result<Self, ProgramError> {
        Ok(Self {
            discriminator: 1,
            address_config: Seed::pack_into_address_config(seeds)?,
            is_signer: is_signer.into(),
            is_writable: is_writable.into(),
        })
    }

    /// Resolve an `ExtraAccountMeta` into an `AccountMeta`, potentially
    /// resolving a program-derived address (PDA) if necessary
    pub fn resolve(
        &self,
        accounts: &[AccountMeta],
        instruction_data: &[u8],
        program_id: &Pubkey,
    ) -> Result<AccountMeta, ProgramError> {
        match self.discriminator {
            0 => AccountMeta::try_from(self),
            1 => {
                let seeds = Seed::unpack_address_config(&self.address_config)?;
                Ok(AccountMeta {
                    pubkey: resolve_pda(&seeds, accounts, instruction_data, program_id)?,
                    is_signer: self.is_signer.into(),
                    is_writable: self.is_writable.into(),
                })
            }
            _ => Err(ProgramError::InvalidAccountData),
        }
    }
}

impl From<&AccountMeta> for ExtraAccountMeta {
    fn from(meta: &AccountMeta) -> Self {
        Self {
            discriminator: 0,
            address_config: meta.pubkey.to_bytes(),
            is_signer: meta.is_signer.into(),
            is_writable: meta.is_writable.into(),
        }
    }
}
impl From<AccountMeta> for ExtraAccountMeta {
    fn from(meta: AccountMeta) -> Self {
        ExtraAccountMeta::from(&meta)
    }
}
impl From<&AccountInfo<'_>> for ExtraAccountMeta {
    fn from(account_info: &AccountInfo) -> Self {
        Self {
            discriminator: 0,
            address_config: account_info.key.to_bytes(),
            is_signer: account_info.is_signer.into(),
            is_writable: account_info.is_writable.into(),
        }
    }
}
impl From<AccountInfo<'_>> for ExtraAccountMeta {
    fn from(account_info: AccountInfo) -> Self {
        ExtraAccountMeta::from(&account_info)
    }
}

impl TryFrom<&ExtraAccountMeta> for AccountMeta {
    type Error = ProgramError;

    fn try_from(pod: &ExtraAccountMeta) -> Result<Self, Self::Error> {
        if pod.discriminator == 0 {
            Ok(AccountMeta {
                pubkey: Pubkey::try_from(pod.address_config)
                    .map_err(|_| ProgramError::from(AccountResolutionError::InvalidPubkey))?,
                is_signer: pod.is_signer.into(),
                is_writable: pod.is_writable.into(),
            })
        } else {
            Err(AccountResolutionError::AccountTypeNotAccountMeta.into())
        }
    }
}