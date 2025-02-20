use evm::{ExitReason, H160, U256};
use solana_program::account_info::AccountInfo;
use solana_program::entrypoint::ProgramResult;
use solana_program::program_error::ProgramError;

use crate::account::{EthereumAccount, Operator, program, State, Treasury};
use crate::account_storage::{AccountsReadiness, ProgramAccountStorage};
use crate::config::{EVM_STEPS_MIN, EVM_STEPS_LAST_ITERATION_MAX, PAYMENT_TO_TREASURE};
use crate::executor::{Action, Gasometer, Machine};
use crate::state_account::Deposit;
use crate::transaction::{check_ethereum_transaction, Transaction};

pub struct Accounts<'a> {
    pub operator: Operator<'a>,
    pub treasury: Treasury<'a>,
    pub operator_ether_account: EthereumAccount<'a>,
    pub system_program: program::System<'a>,
    pub neon_program: program::Neon<'a>,
    pub remaining_accounts: &'a [AccountInfo<'a>],
    pub all_accounts: &'a [AccountInfo<'a>],
}


pub fn do_begin<'a>(
    accounts: Accounts<'a>,
    mut storage: State<'a>,
    account_storage: &mut ProgramAccountStorage<'a>,
    gasometer: Gasometer,
    trx: Transaction,
    caller: H160,
) -> ProgramResult {
    debug_print!("do_begin");

    check_ethereum_transaction(account_storage, &caller, &trx)?;
    account_storage.check_for_blocked_accounts()?;
    account_storage.block_accounts(true);

    let mut executor = Machine::new(caller, account_storage)?;

    if let Some(code_address) = trx.to {
        executor.call_begin(caller, code_address, trx.call_data, trx.value, trx.gas_limit, trx.gas_price)
    } else {
        executor.create_begin(caller, trx.call_data, trx.value, trx.gas_limit, trx.gas_price)
    }?;

    executor.save_into(&mut storage);

    finalize(0, accounts, storage, account_storage, None, gasometer)
}

pub fn do_continue<'a>(
    step_count: u64,
    accounts: Accounts<'a>,
    mut storage: State<'a>,
    account_storage: &mut ProgramAccountStorage<'a>,
    gasometer: Gasometer,
) -> ProgramResult {
    debug_print!("do_continue");

    if (step_count < EVM_STEPS_MIN) && (storage.gas_price > U256::zero()) {
        return Err!(ProgramError::InvalidArgument; "Step limit {step_count} below minimum {EVM_STEPS_MIN}");
    }

    let mut executor = Machine::restore(&storage, account_storage)?;
    let result = execute_steps(&mut executor, step_count);
    let steps_executed = executor.get_steps_executed();

    if steps_executed > 0 {
        executor.save_into(&mut storage);
    }

    let results = match result {
        Ok(()) => None, // step limit
        Err(_) if steps_executed > EVM_STEPS_LAST_ITERATION_MAX => None,
        Err(reason) => Some((reason, executor.into_state_actions())),
    };

    finalize(steps_executed, accounts, storage, account_storage, results, gasometer)
}


type EvmResults = (ExitReason, Vec<Action>);

fn execute_steps(
    executor: &mut Machine<ProgramAccountStorage>,
    step_count: u64,
) -> Result<(), ExitReason> {
    if let Some(reason) = executor.state_mut().exit_reason() {
        if reason != &ExitReason::StepLimitReached {
            debug_print!(
                "Skipping VM execution due to the previous execution result stored to state"
            );
            return Err(*reason);
        }
    }

    let result = executor.execute_n_steps(step_count)
        .map_err(|(_result, exit_reason)| exit_reason);

    if let Err(exit_reason) = &result {
        executor.state_mut().set_exit_reason(Some(*exit_reason));
    }

    result
}

fn pay_gas_cost<'a>(
    used_gas: U256,
    operator_ether_account: EthereumAccount<'a>,
    storage: &mut State<'a>,
    account_storage: &mut ProgramAccountStorage<'a>,
) -> ProgramResult {
    debug_print!("pay_gas_cost {}", used_gas);

    // Can overflow in malicious transaction
    let value = used_gas.saturating_mul(storage.gas_price);
    storage.gas_used = storage.gas_used.saturating_add(used_gas);

    account_storage.transfer_gas_payment(
        storage.caller,
        operator_ether_account,
        value,
    )?;

    Ok(())
}

fn finalize<'a>(
    steps_executed: u64,
    accounts: Accounts<'a>,
    mut storage: State<'a>,
    account_storage: &mut ProgramAccountStorage<'a>,
    results: Option<EvmResults>,
    mut gasometer: Gasometer,
) -> ProgramResult {
    debug_print!("finalize");

    if steps_executed > 0 {
        accounts.system_program.transfer(&accounts.operator, &accounts.treasury, PAYMENT_TO_TREASURE)?;
    }

    let exit_reason_opt = if let Some((exit_reason, apply_state)) = results {
        if account_storage.apply_state_change(
            &accounts.neon_program,
            &accounts.system_program,
            &accounts.operator,
            apply_state,
        )? == AccountsReadiness::Ready {
            Some(exit_reason)
        } else {
            None
        }
    } else {
        None
    };

    gasometer.record_operator_expenses(&accounts.operator);

    let total_used_gas = gasometer.used_gas_total();
    let gas_limit = storage.gas_limit;
    if total_used_gas > gas_limit {
        return Err!(ProgramError::InvalidArgument; "Out of gas used - {total_used_gas}, limit - {gas_limit}")
    }

    let used_gas = gasometer.used_gas();
    solana_program::log::sol_log_data(&[b"IX_GAS", &used_gas.as_u64().to_le_bytes()]);

    pay_gas_cost(used_gas, accounts.operator_ether_account, &mut storage, account_storage)?;


    if let Some(exit_reason) = exit_reason_opt {
        accounts.neon_program.on_return(exit_reason, total_used_gas);

        account_storage.block_accounts(false);
        storage.finalize(Deposit::ReturnToOperator(accounts.operator))?;
    }

    Ok(())
}
