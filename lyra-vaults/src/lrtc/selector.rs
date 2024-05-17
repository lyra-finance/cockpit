use crate::lrtc::params::LRTCParams;
use crate::market::{new_market_state, MarketState};
use anyhow::{Error, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

use log::{debug, error, info, warn};
use lyra_client::auth::{load_signer, sign_auth_header};
use lyra_client::json_rpc::{http_rpc, Notification, Response, WsClient, WsClientExt};
use orderbook_types::types::tickers::result::{
    InstrumentTicker, InstrumentsResponse, OptionType, TickerNotificationData,
};
use serde_json::{json, Value};
use tokio::select;

use crate::shared::{subscribe_tickers, TickerInterval};

/// Returns the option name that satisfies the LRT-C params (target expiry and delta)
pub async fn select_option(params: &LRTCParams) -> Result<String> {
    let market = new_market_state();
    let client = WsClient::new_client().await?;
    let now = chrono::Utc::now().timestamp();
    let options_res = http_rpc::<_, InstrumentsResponse>(
        "public/get_instruments",
        json!({
            "currency": params.currency,
            "instrument_type": "option",
            "expired": false,
        }),
        None,
    )
    .await?
    .into_result()?
    .result;

    let options_iter = options_res.iter().filter(|&r| {
        if let Some(ref details) = r.option_details {
            r.is_active
                && details.option_type.is_call()
                && details.expiry < now + params.expiry.to_expiry_sec()
        } else {
            false
        }
    });

    let err = Error::msg("No options found within the LRTC params");
    let expiry = options_iter.clone().map(|r| r.option_details.as_ref().unwrap().expiry).max();
    let expiry = match expiry {
        Some(expiry) => expiry,
        None => return Err(err),
    };

    let expiry_options: Vec<String> = options_iter
        .filter(|r| r.option_details.as_ref().unwrap().expiry == expiry)
        .map(|r| r.instrument_name.clone())
        .collect();
    let sub = subscribe_tickers(market.clone(), expiry_options, TickerInterval::_1000Ms);
    let _ = select! {
        _ = sub => {},
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {},
    };

    let desired_delta = params.delta.to_decimal();
    let reader = market.read().await;
    let selected_option = reader
        .iter_tickers()
        .filter(|&ticker| {
            if let Some(ref pricing) = ticker.option_pricing {
                pricing.delta < desired_delta
            } else {
                false
            }
        })
        .min_by_key(|&ticker| {
            (ticker.option_pricing.as_ref().unwrap().delta.clone() - &desired_delta).abs()
        });
    match selected_option {
        Some(option) => Ok(option.instrument_name.clone()),
        None => Err(err),
    }
}
