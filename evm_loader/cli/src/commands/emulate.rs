use log::{debug};

use ethnum::U256;
use evm_loader::{
    gasometer::LAMPORTS_PER_SIGNATURE, 
    config::{EVM_STEPS_MIN, PAYMENT_TO_TREASURE},
    types::{Address, Transaction}, executor::ExecutorState, evm::{Machine, ExitStatus}
};

use crate::{
    account_storage::{
        EmulatorAccountStorage, NeonAccount, SolanaAccount,
    },
    Config,
    NeonCliResult,
    syscall_stubs::Stubs, errors::NeonCliError,
};

use solana_sdk::pubkey::Pubkey;
use evm_loader::account_storage::AccountStorage;

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn execute(
    config: &Config, 
    contract_id: Option<Address>, 
    caller_id: Address, 
    data: Option<Vec<u8>>,
    value: Option<U256>,
    token_mint: &Pubkey,
    chain_id: u64,
    max_steps_to_execute: u64,
) -> NeonCliResult {
    debug!("command_emulate(config={:?}, contract_id={:?}, caller_id={:?}, data={:?}, value={:?})",
        config,
        contract_id,
        caller_id,
        &hex::encode(data.clone().unwrap_or_default()),
        value);

    let syscall_stubs = Stubs::new(config)?;
    solana_sdk::program_stubs::set_syscall_stubs(syscall_stubs);

    let storage = EmulatorAccountStorage::new(config, *token_mint, chain_id);

    let trx = Transaction {
        nonce: storage.nonce(&caller_id),
        gas_price: U256::ZERO,
        gas_limit: U256::MAX,
        target: contract_id,
        value: value.unwrap_or_default(),
        call_data: data.unwrap_or_default(),
        chain_id: Some(chain_id.into()),
        ..Transaction::default()
    };

    let (exit_status, actions, steps_executed) = {
        let mut backend = ExecutorState::new(&storage);
        let mut evm = Machine::new(trx, caller_id, &mut backend)?;

        let (result, steps_executed) = evm.execute(max_steps_to_execute, &mut backend)?;
        let actions = backend.into_actions();
        (result, actions, steps_executed)
    };

    debug!("Execute done, result={exit_status:?}");
    debug!("{steps_executed} steps executed");

    if exit_status == ExitStatus::StepLimit {
        return Err(NeonCliError::TooManySteps);
    }


    let accounts_operations = storage.calc_accounts_operations(&actions);

    let max_iterations = (steps_executed + (EVM_STEPS_MIN - 1)) / EVM_STEPS_MIN;
    let steps_gas = max_iterations * (LAMPORTS_PER_SIGNATURE + PAYMENT_TO_TREASURE);
    let begin_end_gas = 2 * LAMPORTS_PER_SIGNATURE;
    let actions_gas = storage.apply_actions(actions);
    let accounts_gas = storage.apply_accounts_operations(accounts_operations);
    debug!("Gas - steps: {steps_gas}, actions: {actions_gas}, accounts: {accounts_gas}");

    let (result, status) = match exit_status {
        ExitStatus::Return(v) => (v, "succeed"),
        ExitStatus::Revert(v) => (v, "revert"),
        ExitStatus::Stop | ExitStatus::Suicide => (vec![], "succeed"),
        ExitStatus::StepLimit => unreachable!(),
    };

    let accounts: Vec<NeonAccount> = storage.accounts
        .borrow()
        .values()
        .cloned()
        .collect();

    let solana_accounts: Vec<SolanaAccount> = storage.solana_accounts
        .borrow()
        .values()
        .cloned()
        .collect();

    let js = serde_json::json!({
        "accounts": accounts,
        "solana_accounts": solana_accounts,
        "token_accounts": [],
        "result": hex::encode(result),
        "exit_status": status,
        "steps_executed": steps_executed,
        "used_gas": steps_gas + begin_end_gas + actions_gas + accounts_gas
    });

    println!("{}", js);

    Ok(())
}
