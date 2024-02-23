use crate::api::{
	binance::{self},
	order_types::*,
	Market, Symbol,
};
use crate::positions::PositionSpec;
use crate::protocols::{FollowupProtocol, ProtocolCache, ProtocolType};
use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use v_utils::macros::CompactFormat;
use v_utils::trades::Side;

#[derive(Debug, Clone, CompactFormat)]
pub struct TrailingStop {
	pub percent: f64,
}
impl FollowupProtocol for TrailingStop {
	type Cache = TrailingStopCache;

	async fn attach<T>(&self, orders: &mut Vec<OrderTypeP>, cache: &mut Self::Cache) -> Result<()> {
		let address = format!("wss://fstream.binance.com/ws/{}@markPrice", &cache.symbol);
		let url = url::Url::parse(&address).unwrap();
		let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
		let (_, read) = ws_stream.split();

		read.for_each(|message| {
			let cache_blob = cache_blob.clone();
			async move {
				let data = message.unwrap().into_data();
				match serde_json::from_slice::<Value>(&data) {
					Ok(json) => {
						if let Some(price_str) = json.get("p") {
							let price: f64 = price_str.as_str().unwrap().parse().unwrap();
							if price < cache.bottom {
								cache.bottom = price;
								match side {
									Side::Buy => {}
									Side::Sell => {
										let target_price = price + price * self.percent;
										orders.clear();
										orders.push(StopMarketWhere {
											symbol: cache.symbol,
											side: Side::Buy,
											price: target_price,
										});
									}
								}
							}
							if price > cache.top {
								cache.top = price;
								match side {
									Side::Buy => {
										let target_price = price - price * self.percent;
										orders.clear();
										orders.push(StopMarketWhere {
											symbol: cache.symbol,
											side: Side::Sell,
											price: target_price,
										});
									}
									Side::Sell => {}
								}
							}
						}
					}
					Err(e) => {
						println!("Failed to parse message as JSON: {}", e);
					}
				}
			}
		})
		.await;
	}

	fn subtype(&self) -> ProtocolType {
		ProtocolType::Momentum
	}
}

/// Stores both highest and lowest prices in case the direction is switched for some reason. Note: it's not meant to though.
#[derive(Debug)]
pub struct TrailingStopCache {
	pub symbol: Symbol,
	pub top: f64,
	pub bottom: f64,
}
impl ProtocolCache for TrailingStopCache {
	async fn build<T>(_spec: T, position_core: &PositionSpec) -> Result<Self> {
		let binance_symbol = Symbol {
			base: position_core.asset.clone(),
			quote: "USDT".to_owned(),
			market: Market::BinanceFutures,
		};
		let price = binance::futures_price(&binance_symbol.base).await?;
		Ok(Self {
			symbol: binance_symbol,
			top: price,
			bottom: price,
		})
	}
}
