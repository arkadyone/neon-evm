use std::convert::TryInto;

use solana_sdk::{ pubkey::Pubkey };

use ethnum::U256;

use evm_loader::{
    account::{EthereumStorage, ACCOUNT_SEED_VERSION},
    config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
    types::Address,
};
use evm_loader::account::EthereumAccount;

use crate::{
    account_storage::{EmulatorAccountStorage, account_info },
    Config,
};



pub fn execute(
    config: &Config,
    ether_address: Address,
    index: &U256
) {
    let value = if let (solana_address, Some(mut account)) = EmulatorAccountStorage::get_account_from_solana(config, &ether_address) {
        let info = account_info(&solana_address, &mut account);

        let account_data = EthereumAccount::from_account(&config.evm_loader, &info).unwrap();
        if let Some(contract) = account_data.contract_data() {
            if *index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT) {
                let index: usize = index.as_usize() * 32;
                contract.storage()[index..index + 32].try_into().unwrap()
            } else {
                let subindex = (*index & 0xFF).as_u8();
                let index = *index & !U256::new(0xFF);

                let index_bytes = index.to_le_bytes();
                let seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], b"ContractStorage", ether_address.as_bytes(), &account_data.generation.to_le_bytes(), &index_bytes];

                let (address, _) = Pubkey::find_program_address(seeds, &config.evm_loader);

                if let Ok(mut account) = config.rpc_client.get_account(&address) {
                    if solana_sdk::system_program::check_id(&account.owner) {
                        <[u8; 32]>::default()
                    } else {
                        let account_info = account_info(&address, &mut account);
                        let storage = EthereumStorage::from_account(&config.evm_loader, &account_info).unwrap();
                        storage.get(subindex)
                    }
                } else {
                    <[u8; 32]>::default()
                }
            }
        } else {
            <[u8; 32]>::default()
        }
    } else {
        <[u8; 32]>::default()
    };

    print!("{}", hex::encode(value));
}
