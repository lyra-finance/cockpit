use crate::lrtc::params::OptionAuctionParams;
use crate::shared::auction::LimitOrderAuctionExecutor;
use crate::shared::params::SpotAuctionParams;
use crate::shared::stages::{TSACollateralOnly, TSAWaitForSettlement};
use std::fmt::Debug;

#[derive(Debug)]
pub enum LRTCExecutorStage {
    SpotOnly(TSACollateralOnly),
    OptionAuction(LimitOrderAuctionExecutor<OptionAuctionParams>),
    AwaitSettlement(TSAWaitForSettlement),
    SpotAuction(LimitOrderAuctionExecutor<SpotAuctionParams>),
}
