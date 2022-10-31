use std::convert::TryInto;

use super::{program, EthereumStorage, Operator, Packable};
use solana_program::{program_error::ProgramError, rent::Rent, sysvar::Sysvar};

/// Ethereum storage data account
#[derive(Default, Debug)]
pub struct Data {}

impl Packable for Data {
    /// Storage struct tag
    const TAG: u8 = super::TAG_CONTRACT_STORAGE;
    /// Storage struct serialized size
    const SIZE: usize = 0;

    /// Deserialize `Storage` struct from input data
    #[must_use]
    fn unpack(_src: &[u8]) -> Self {
        Self {}
    }

    /// Serialize `Storage` struct into given destination
    fn pack(&self, _dst: &mut [u8]) {}
}

impl<'a> EthereumStorage<'a> {
    #[must_use]
    pub fn get(&self, subindex: u8) -> [u8; 32] {
        let data = self.info.data.borrow();
        let data = &data[1..]; // skip tag

        for chunk in data.chunks_exact(1 + 32) {
            if chunk[0] != subindex {
                continue;
            }

            return chunk[1..].try_into().unwrap();
        }

        [0_u8; 32]
    }

    pub fn set(
        &mut self,
        subindex: u8,
        value: &[u8; 32],
        operator: &Operator<'a>,
        system: &program::System<'a>,
    ) -> Result<(), ProgramError> {
        {
            let mut data = self.info.data.borrow_mut();
            let data = &mut data[1..]; // skip tag

            for chunk in data.chunks_exact_mut(1 + 32) {
                if chunk[0] != subindex {
                    continue;
                }

                chunk[1..].copy_from_slice(value);

                return Ok(());
            }
        } // drop `data`

        let new_len = self.info.data_len() + 1 + 32; // new_len <= 8.25 kb
        self.info.realloc(new_len, false)?;

        let minimum_balance = Rent::get()?.minimum_balance(new_len);
        if self.info.lamports() < minimum_balance {
            let required_lamports = minimum_balance - self.info.lamports();
            system.transfer(operator, self.info, required_lamports)?;
        }

        let mut data = self.info.data.borrow_mut();
        let data = &mut data[1..]; // skip tag

        let chunk_start = data.len() - 1 - 32;
        let chunk = &mut data[chunk_start..];

        chunk[0] = subindex;
        chunk[1..].copy_from_slice(value);

        Ok(())
    }
}
