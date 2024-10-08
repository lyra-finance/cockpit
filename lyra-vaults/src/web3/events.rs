use crate::web3::contracts::get_tsa_contract;
use crate::web3::{ProviderWithSigner, GAS_FACTOR, GAS_PRICE, TSA};
use anyhow::{Error, Result};
use ethers::prelude::{Middleware, ValueOrArray, U256, U64};
use log::{debug, info};

pub const MAX_TO_PROCESS_PER_CALL: usize = 32;

pub async fn process_deposit_events(tsa: &TSA<ProviderWithSigner>) -> Result<()> {
    let block = tsa.client().get_block_number().await?;
    // assume all deposits outside of this range are already processed
    let from = block - U64::from(100_000);
    let addr = ValueOrArray::Value(tsa.address());
    let init_filter = tsa.deposit_initiated_filter().from_block(from).address(addr.clone());
    let proc_filter = tsa.deposit_processed_filter().from_block(from).address(addr);

    info!("Running deposit queries");
    let inits: Vec<U256> = init_filter.query().await?.iter().map(|e| e.deposit_id).collect();
    info!("Deposits initiated: {:?}", inits);
    let procs: Vec<U256> = proc_filter.query().await?.iter().map(|e| e.deposit_id).collect();
    info!("Deposits processed: {:?}", procs);

    let pending: Vec<U256> = inits.into_iter().filter(|i| !procs.contains(i)).collect();
    info!("Pending deposits: {:?}", pending);
    if pending.is_empty() {
        info!("No pending deposits");
        return Ok(());
    }
    let pending = pending.into_iter().take(MAX_TO_PROCESS_PER_CALL).collect();
    info!("Processing subset of deposits: {:?}", pending);

    let call = tsa.process_deposits(pending).gas_price(GAS_PRICE);
    let gas = call.estimate_gas().await? * U256::from(GAS_FACTOR);
    let call = call.gas(gas);
    let pending_tx = call.send().await?;
    let receipt = pending_tx.await?.ok_or(Error::msg("Failed"))?;
    info!("Tx receipt: {}", serde_json::to_string(&receipt)?);
    let tx = tsa.client().get_transaction(receipt.transaction_hash).await?;
    info!("Initiate deposit tx: {:?}", tx);
    Ok(())
}
