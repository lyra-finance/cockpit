extern crate core;

mod helpers;
mod lrtc;
mod market;
mod shared;
mod web3;

use crate::lrtc::executor::LRTCExecutor;
use crate::lrtc::stages::LRTCStage;
use crate::web3::{actions, events, get_subaccount_id};
use anyhow::{Error, Result};
use bigdecimal::BigDecimal;
use ethers::abi::Address;
use log::{debug, error, info, warn};
use lrtc::params::{LRTCParams, OptionAuctionParams};
use lyra_client::setup::{ensure_session_key, setup_env};
use serde::{Deserialize, Serialize};
use shared::params::SpotAuctionParams;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::{join, select, try_join};
use web3::scripts;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum VaultParams {
    LRTC(LRTCParams),
    // Add more vaults here
}

async fn run_lrtc(params: LRTCParams) -> Result<()> {
    let vault_name = params.vault_name.clone();
    std::env::set_var("ENV", params.env.clone());
    std::env::set_var("SESSION_KEY_NAME", vault_name.to_lowercase());
    println!("Setting up {} env for LRTC executor", params.env.clone());
    setup_env().await;
    ensure_session_key().await;
    info!("LRTC executor params: {:?}", params);

    let subacc_id = get_subaccount_id(&vault_name).await?;
    info!("Vault Subaccount ID: {}", subacc_id);
    std::env::set_var("SUBACCOUNT_ID", subacc_id.to_string());
    std::env::set_var("VAULT_NAME", vault_name.clone());
    std::env::set_var("SPOT_NAME", params.option_auction_params.spot_name.clone());
    std::env::set_var("CASH_NAME", params.spot_auction_params.cash_name.clone());

    let tsa_address: String = std::env::var(format!("{vault_name}_TSA_ADDRESS")).unwrap();
    std::env::set_var("OWNER_PUBLIC_KEY", tsa_address);

    info!("Starting LRTC executor");
    let mut executor = LRTCExecutor::new(params).await?;
    let task_handle = tokio::spawn(async move { executor.run().await });
    let res = task_handle.await?;
    if let Err(e) = res {
        error!("Executor failed: {:?}", e);
    }
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    println!("Reading params from json file");
    // read json name from cmd input
    let args: Vec<String> = std::env::args().collect();
    let json_name = args.get(1).ok_or(Error::msg("No json name provided"))?;
    let params = tokio::fs::read_to_string(format!("./params/{json_name}.json")).await?;
    let params: VaultParams = serde_json::from_str(&params)?;
    match params {
        VaultParams::LRTC(params) => run_lrtc(params).await?,
    }

    Ok(())
}
