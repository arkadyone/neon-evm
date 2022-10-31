use std::convert::From;
use solana_program::account_info::AccountInfo;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;
use solana_program::{
    program::{invoke, invoke_signed}, system_instruction,
    rent::Rent, sysvar::Sysvar
};
use super::{Operator};
use std::ops::Deref;


pub struct Neon<'a> (&'a AccountInfo<'a>);

impl<'a> Neon<'a> {
    pub fn from_account(program_id: &Pubkey, info: &'a AccountInfo<'a>) -> Result<Self, ProgramError> {
        if program_id != info.key {
            return Err!(ProgramError::InvalidArgument; "Account {} - is not Neon program", info.key);
        }

        Ok(Self ( info ))
    }
}

impl<'a> Deref for Neon<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}


pub struct System<'a> (&'a AccountInfo<'a>);

impl<'a> From<&'a AccountInfo<'a>> for System<'a> {
    fn from(info: &'a AccountInfo<'a>) -> Self {
        Self( info )
    }
}

impl<'a> From<& System<'a>> for &'a AccountInfo<'a> {
    fn from(f:& System<'a>) -> Self {
        f.0
    }
}

impl<'a> System<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self, ProgramError> {
        if !solana_program::system_program::check_id(info.key) {
            return Err!(ProgramError::InvalidArgument; "Account {} - is not system program", info.key);
        }

        Ok(Self ( info ))
    }

    pub fn create_pda_account(
        &self,
        program_id: &Pubkey,
        payer: &Operator<'a>,
        new_account: &AccountInfo<'a>,
        new_account_seeds: &[&[u8]],
        space: usize
    ) -> Result<(), ProgramError> {
        let rent = Rent::get()?;
        let minimum_balance = rent.minimum_balance(space).max(1);

        if new_account.lamports() > 0 {
            let required_lamports = minimum_balance.saturating_sub(new_account.lamports());

            if required_lamports > 0 {
                invoke(
                    &system_instruction::transfer(payer.key, new_account.key, required_lamports),
                    &[(*payer).clone(), new_account.clone(), self.0.clone()]
                )?;
            }

            invoke_signed(
                &system_instruction::allocate(new_account.key, space as u64),
                &[new_account.clone(), self.0.clone()],
                &[new_account_seeds],
            )?;

            invoke_signed(
                &system_instruction::assign(new_account.key, program_id),
                &[new_account.clone(), self.0.clone()],
                &[new_account_seeds]
            )
        } else {
            invoke_signed(
                &system_instruction::create_account(
                    payer.key,
                    new_account.key,
                    minimum_balance,
                    space as u64,
                    program_id,
                ),
                &[(*payer).clone(), new_account.clone(), self.0.clone()],
                &[new_account_seeds],
            )
        }
    }

    pub fn transfer(
        &self,
        source: &Operator<'a>,
        target: &AccountInfo<'a>,
        lamports: u64
    ) -> Result<(), ProgramError> {
        crate::debug_print!("system transfer {} lamports from {} to {}", lamports, source.key, target.key);

        invoke(
            &system_instruction::transfer(source.key, target.key, lamports),
            &[(*source).clone(), target.clone(), self.0.clone()]
        )
    }
}

impl<'a> Deref for System<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}


pub struct Token<'a>(&'a AccountInfo<'a>);

impl<'a> Token<'a> {
    pub fn from_account(info: &'a AccountInfo<'a>) -> Result<Self, ProgramError> {
        if !spl_token::check_id(info.key) {
            return Err!(ProgramError::InvalidArgument; "Account {} - is not token program", info.key);
        }

        Ok(Self ( info ))
    }

    pub fn create_account(
        &self,
        account: &AccountInfo<'a>,
        mint: &AccountInfo<'a>,
        owner: &AccountInfo<'a>
    ) -> Result<(), ProgramError> {
        invoke(
            &spl_token::instruction::initialize_account3(
                self.0.key,
                account.key,
                mint.key,
                owner.key
            )?,
            &[account.clone(), mint.clone()]
        )
    }

}

impl<'a> Deref for Token<'a> {
    type Target = AccountInfo<'a>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
