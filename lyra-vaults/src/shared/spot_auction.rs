use crate::shared::auction::{LimitOrderAuction, OrderStrategy};
use crate::shared::params::SpotAuctionParams;
use anyhow::{Error, Result};
use bigdecimal::{BigDecimal, FromPrimitive, RoundingMode, ToPrimitive, Zero};
use log::{debug, info};
use lyra_client::actions::Direction;
use lyra_utils::black76::OptionContract;
use orderbook_types::types::tickers::OptionType;
use std::cmp::Ordering;
impl OrderStrategy for SpotAuctionParams {
    async fn get_desired_price(&self, auction: &LimitOrderAuction) -> Result<BigDecimal> {
        let market = &auction.market;
        let reader = market.read().await;
        let ticker =
            reader.get_ticker(&auction.instrument_name).ok_or(Error::msg("Ticker not found"))?;
        let cash_pos = reader.get_position(&self.cash_name);
        let zero = BigDecimal::zero();
        if cash_pos.is_none() {
            return Ok(zero);
        }
        let cash_pos = cash_pos.unwrap();

        let direction = match cash_pos.amount.cmp(&zero) {
            Ordering::Less => Direction::Sell, // neg cash -> sell LRTs
            Ordering::Greater => Direction::Buy,
            Ordering::Equal => {
                return Ok(zero);
            }
        };

        let spread = self.get_spot_spread(auction.start_timestamp_sec);
        let spot = ticker.mark_price.to_f64().ok_or(Error::msg("spot cast to f64 failed"))?;

        debug!("SpotAuction spot, spread: {}, {}", spot, spread);

        let price = match direction {
            Direction::Buy => spot * (1.0 + spread),
            Direction::Sell => spot * (1.0 - spread),
        };

        let price = BigDecimal::from_f64(price)
            .unwrap()
            .round(ticker.tick_size.fractional_digit_count())
            .max(ticker.min_price.clone());

        Ok(price)
    }
    async fn get_desired_amount(
        &self,
        auction: &LimitOrderAuction,
        price: &BigDecimal,
    ) -> Result<(Direction, BigDecimal)> {
        let market = &auction.market;
        let reader = market.read().await;
        let ticker =
            reader.get_ticker(&auction.instrument_name).ok_or(Error::msg("Ticker not found"))?;
        let cash_pos = reader.get_position(&self.cash_name);
        let zero = BigDecimal::zero();
        if cash_pos.is_none() || price == &zero {
            return Ok((Direction::Sell, zero));
        }
        let cash_pos = cash_pos.unwrap();
        if auction.remain_sec() <= 0 && cash_pos.amount > -&self.max_cash {
            return Ok((Direction::Sell, zero));
        }

        let amount = &cash_pos.amount / price;
        let (direction, amount) = match &amount.cmp(&zero) {
            Ordering::Less | Ordering::Equal => (Direction::Sell, -amount),
            Ordering::Greater => (Direction::Buy, amount),
        };

        // when selling LRTs, ok to sell a tiny bit more to cover neg cash
        // let mode = match direction {
        //     Direction::Buy => RoundingMode::Down,
        //     Direction::Sell => RoundingMode::Up,
        // };
        // todo want to use above logic but running into reverts due to spotTransactionLeniency
        // need to do round followed by cap/floor at the 1.05 / 0.95
        let mode = RoundingMode::Down;

        let amount = amount.with_scale_round(ticker.amount_step.fractional_digit_count(), mode);
        if amount < ticker.minimum_amount.clone() {
            return Ok((Direction::Sell, zero));
        }
        Ok((direction, amount))
    }
}
