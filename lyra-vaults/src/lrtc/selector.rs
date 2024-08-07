use crate::lrtc::params::LRTCParams;
use crate::market::{new_market_state, MarketState};
use anyhow::{Error, Result};
use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};

use log::{debug, error, info, warn};
use lyra_client::auth::{load_signer, sign_auth_header};
use lyra_client::json_rpc::{http_rpc, Notification, Response, WsClient, WsClientExt};
use orderbook_types::types::tickers::result::{
    InstrumentTicker, InstrumentsResponse, OptionType, TickerNotificationData,
};
use serde_json::{json, Value};
use tokio::select;

use crate::helpers::{get_expiry_options, subscribe_tickers, sync_subaccount, TickerInterval};

/// Returns the option name that satisfies the LRT-C params (target expiry and delta)
pub async fn select_new_option(params: &LRTCParams) -> Result<String> {
    let market = new_market_state();
    let client = WsClient::new_client().await?;
    let now = chrono::Utc::now().timestamp();
    let err = Error::msg("No options found within the LRTC params");

    let expiry_options = get_expiry_options(
        &params.option_currency,
        params.expiry_sec(),
        params.min_expiry_sec(),
        true,
    )
    .await?;

    let sub = subscribe_tickers(market.clone(), expiry_options, TickerInterval::_1000Ms);
    let _ = select! {
        _ = sub => {},
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {},
    };

    let desired_delta = &params.target_delta;
    let reader = market.read().await;
    let selected_option = reader
        .iter_tickers()
        .filter(|&ticker| {
            if let Some(ref pricing) = ticker.option_pricing {
                &pricing.delta < &params.max_delta
            } else {
                false
            }
        })
        .min_by_key(|&ticker| {
            (ticker.option_pricing.as_ref().unwrap().delta.clone() - desired_delta).abs()
        });
    match selected_option {
        Some(option) => Ok(option.instrument_name.clone()),
        None => Err(err),
    }
}

/// Returns the option name from an existing position
/// Expects the market state to be synced to the subaccount
pub async fn maybe_select_from_positions(market: &MarketState) -> Result<Option<String>> {
    let reader = market.read().await;
    let position_names: Vec<String> = reader
        .iter_positions()
        .filter(|&p| {
            p.amount != BigDecimal::zero()
                && (p.instrument_name.ends_with("-C") || p.instrument_name.ends_with("-P"))
        })
        .map(|p| p.instrument_name.clone())
        .collect();
    match position_names.len() {
        0 => Ok(None),
        1 => Ok(Some(position_names[0].clone())),
        _ => Err(Error::msg("Unexpected multiple open options positions")),
    }
}
