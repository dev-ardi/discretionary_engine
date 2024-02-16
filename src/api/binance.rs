#![allow(non_snake_case, dead_code)]
use crate::api::{Market, OrderType};
use crate::protocols::Klines;
use anyhow::Result;
use arrow2::array::{Float64Array, Int64Array};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_json::Value;
use serde_urlencoded;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use url::Url;
use v_utils::trades::{Side, Timeframe};

type HmacSha256 = Hmac<Sha256>;

#[allow(dead_code)]
pub enum HttpMethod {
	GET,
	POST,
	PUT,
	DELETE,
}

#[allow(dead_code)]
pub struct Binance {
	// And so then many calls will be replaced with just finding info here.
	futures_symbols: HashMap<String, FuturesSymbol>,
}

pub async fn signed_request(
	http_method: HttpMethod,
	endpoint_str: &str,
	mut params: HashMap<&'static str, String>,
	key: String,
	secret: String,
) -> Result<reqwest::Response> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json;charset=utf-8"));
	headers.insert("X-MBX-APIKEY", HeaderValue::from_str(&key).unwrap());
	let client = reqwest::Client::builder().default_headers(headers).build()?;

	let time_ms = Utc::now().timestamp_millis();
	params.insert("timestamp", format!("{}", time_ms));

	let query_string = serde_urlencoded::to_string(&params)?;

	let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
	mac.update(query_string.as_bytes());
	let mac_bytes = mac.finalize().into_bytes();
	let signature = hex::encode(mac_bytes);

	let url = format!("{}?{}&signature={}", endpoint_str, query_string, signature);

	let r = match http_method {
		HttpMethod::GET => client.get(&url).send().await?,
		HttpMethod::POST => client.post(&url).send().await?,
		_ => panic!("Not implemented"),
	};
	Ok(r)
}

/// All the iteractions with submitting orders use this
pub enum BinanceOrder {
	Market,
	Limit,
	StopLoss,
	StopLossLimit,
	TakeProfit,
	TakeProfitLimit,
	LimitMaker,
}
impl From<OrderType> for BinanceOrder {
	fn from(order_type: OrderType) -> Self {
		match order_type {
			OrderType::Market => unimplemented!(),
			OrderType::Limit => unimplemented!(),
			OrderType::StopMarket => unimplemented!(),
			OrderType::StopLimit => unimplemented!(),
			OrderType::TakeProfit => unimplemented!(),
			OrderType::TakeProfitLimit => unimplemented!(),
			OrderType::LimitMaker => unimplemented!(),
		}
	}
}
impl ToString for BinanceOrder {
	fn to_string(&self) -> String {
		match self {
			BinanceOrder::Market => "MARKET".to_string(),
			BinanceOrder::Limit => "LIMIT".to_string(),
			BinanceOrder::StopLoss => "STOP_LOSS".to_string(),
			BinanceOrder::StopLossLimit => "STOP_LOSS_LIMIT".to_string(),
			BinanceOrder::TakeProfit => "TAKE_PROFIT".to_string(),
			BinanceOrder::TakeProfitLimit => "TAKE_PROFIT_LIMIT".to_string(),
			BinanceOrder::LimitMaker => "LIMIT_MAKER".to_string(),
		}
	}
}

pub async fn get_balance(key: String, secret: String, market: Market) -> Result<f64> {
	let params = HashMap::<&str, String>::new();
	match market {
		Market::BinanceFutures => {
			let base_url = market.get_base_url();
			let url = base_url.join("fapi/v2/balance")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let asset_balances: Vec<FuturesBalance> = r.json().await?;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.balance.parse::<f64>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceSpot => {
			let base_url = market.get_base_url();
			let url = base_url.join("/api/v3/account")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let account_details: SpotAccountDetails = r.json().await?;
			let asset_balances = account_details.balances;

			let mut total_balance = 0.0;
			for asset in asset_balances {
				total_balance += asset.free.parse::<f64>()?;
				total_balance += asset.locked.parse::<f64>()?;
			}
			Ok(total_balance)
		}
		Market::BinanceMargin => {
			let base_url = market.get_base_url();
			let url = base_url.join("/sapi/v1/margin/account")?;

			let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
			let account_details: MarginAccountDetails = r.json().await?;
			let total_balance: f64 = account_details.TotalCollateralValueInUSDT.parse()?;

			Ok(total_balance)
		}
	}
}

pub async fn futures_price(asset: &str) -> Result<f64> {
	let symbol = Symbol {
		base: asset,
		quote: "USDT".to_string(),
		market: Market::BinanceFutures,
	};
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v2/ticker/price")?;

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol.clone());

	let client = reqwest::Client::new();
	let r = client.get(url).json(&params).send().await?;
	//let r_json: serde_json::Value = r.json().await?;
	//let price = r_json.get("price").unwrap().as_str().unwrap().parse::<f64>()?;
	// for some reason, can't sumbit with the symbol, so effectively requesting all for now
	let prices: Vec<serde_json::Value> = r.json().await?;
	let price = prices
		.iter()
		.find(|x| x.get("symbol").unwrap().as_str().unwrap().to_string() == symbol)
		.unwrap()
		.get("price")
		.unwrap()
		.as_str()
		.unwrap()
		.parse::<f64>()?;

	Ok(price)
}

pub async fn get_futures_positions(key: String, secret: String) -> Result<HashMap<String, f64>> {
	let url = FuturesAllPositionsResponse::get_url();

	let r = signed_request(HttpMethod::GET, url.as_str(), HashMap::new(), key, secret).await?;
	let positions: Vec<FuturesAllPositionsResponse> = r.json().await?;

	let mut positions_map = HashMap::<String, f64>::new();
	for position in positions {
		let symbol = position.symbol.clone();
		let qty = position.positionAmt.parse::<f64>()?;
		positions_map.entry(symbol).and_modify(|e| *e += qty).or_insert(qty);
	}
	Ok(positions_map)
}

pub async fn futures_quantity_precision(symbol: String) -> Result<usize> {
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v1/exchangeInfo")?;

	let r = reqwest::get(url).await?;
	let futures_exchange_info: FuturesExchangeInfo = r.json().await?;
	let symbol_info = futures_exchange_info.symbols.iter().find(|x| x.symbol == symbol).unwrap();

	Ok(symbol_info.quantityPrecision)
}

/// submits an order, if successful, returns the order id
//TODO!!: make the symbol be from utils \
pub async fn post_futures_order(key: String, secret: String, order_type: OrderType, symbol: String, side: Side, quantity: f64) -> Result<i64> {
	let url = FuturesPositionResponse::get_url();

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol);
	params.insert("side", side.to_string());
	params.insert("type", order_type.to_string());
	params.insert("quantity", format!("{}", quantity));

	let r = signed_request(HttpMethod::POST, url.as_str(), params, key, secret).await?;
	let response: FuturesPositionResponse = r.json().await?;
	Ok(response.orderId)
}

/// Normally, the only cases where the return from this poll is going to be _reacted_ to, is when response.status == OrderStatus::Filled or an error is returned.
pub async fn poll_futures_order(key: String, secret: String, order_id: i64, symbol: String) -> Result<FuturesPositionResponse> {
	let url = FuturesPositionResponse::get_url();

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", format!("{}", symbol));
	params.insert("orderId", format!("{}", order_id));

	let r = signed_request(HttpMethod::GET, url.as_str(), params, key, secret).await?;
	let response: FuturesPositionResponse = r.json().await?;
	Ok(response)
}

pub async fn get_futures_klines(symbol: String, timeframe: Timeframe, limit: usize) -> Result<Klines> {
	assert!(limit <= 1500);
	let base_url = Market::BinanceFutures.get_base_url();
	let url = base_url.join("/fapi/v1/klines")?;

	let mut params = HashMap::<&str, String>::new();
	params.insert("symbol", symbol);
	params.insert("interval", timeframe.to_string());
	params.insert("limit", format!("{}", limit));

	let client = reqwest::Client::new();
	let r = client.get(url).json(&params).send().await?;
	let response_klines: Vec<ResponseKline> = r.json().await?;

	let mut t_open = Vec::new();
	let mut open = Vec::new();
	let mut high = Vec::new();
	let mut low = Vec::new();
	let mut close = Vec::new();
	let mut volume = Vec::new();
	for kline in response_klines {
		t_open.push(Some(kline.open_time));
		open.push(Some(kline.open.parse::<f64>().unwrap()));
		high.push(Some(kline.high.parse::<f64>().unwrap()));
		low.push(Some(kline.low.parse::<f64>().unwrap()));
		close.push(Some(kline.close.parse::<f64>().unwrap()));
		volume.push(Some(kline.volume.parse::<f64>().unwrap()));
	}
	let klines = Klines {
		t_open: Int64Array::from(t_open),
		open: Float64Array::from(open),
		high: Float64Array::from(high),
		low: Float64Array::from(low),
		close: Float64Array::from(close),
		volume: Some(Float64Array::from(volume)),
	};
	Ok(klines)
}

//async fn binance_websocket_klines(klines_arc: Arc<Mutex<Klines>>, symbol: String, timeframe: Timeframe) {{{{
//	let address = "wss://fstream.binance.com/ws/btcusdt@markPrice";
//	let url = url::Url::parse(address).unwrap();
//	let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
//	let (_, read) = ws_stream.split();
//
//	read.for_each(|message| {
//		let main_line = self_arc.clone(); // Cloning the Arc for each iteration
//		let output = output.clone(); // Can i get rid of these?
//		async move {
//			let data = message.unwrap().into_data();
//			match serde_json::from_slice::<Value>(&data) {
//				Ok(json) => {
//					if let Some(price_str) = json.get("p") {
//						let price: f64 = price_str.as_str().unwrap().parse().unwrap();
//						let mut main_line = main_line.lock().unwrap();
//						main_line.btcusdt = Some(price);
//						let mut output_lock = output.lock().unwrap();
//						output_lock.main_line_str = main_line.display(config);
//						output_lock.out().unwrap();
//					}
//				}
//				Err(e) => {
//					println!("Failed to parse message as JSON: {}", e);
//				}
//			}
//		}
//	})
//	.await;
//}}}}
//=============================================================================
// Response structs {{{
//=============================================================================

//? Should I be doing `impl get_url` on these? Unless we have high degree of shared feilds between the markets, this is a big "YES".
//? What if in cases when the struct is shared, I just implement market_specific commands to retrieve the url?
// Trying this out now. So far so good.

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum OrderStatus {
	#[serde(rename = "NEW")]
	New,
	#[serde(rename = "PARTIALLY_FILLED")]
	PartiallyFilled,
	#[serde(rename = "FILLED")]
	Filled,
	#[serde(rename = "CANCELED")]
	Canceled,
	#[serde(rename = "EXPIRED")]
	Expired,
	#[serde(rename = "EXPIRED_IN_MATCH")]
	ExpiredInMatch,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FuturesPositionResponse {
	pub clientOrderId: Option<String>,
	pub cumQty: Option<String>,
	pub cumQuote: String,
	pub executedQty: String,
	pub orderId: i64,
	pub avgPrice: Option<String>,
	pub origQty: String,
	pub price: String,
	pub reduceOnly: Value,
	pub side: String,
	pub positionSide: Option<String>, // only sent when in hedge mode
	pub status: OrderStatus,
	pub stopPrice: String,
	pub closePosition: Value,
	pub symbol: String,
	pub timeInForce: String,
	pub r#type: String,
	pub origType: String,
	pub activatePrice: Option<f64>, // only returned on TRAILING_STOP_MARKET order
	pub priceRate: Option<f64>,     // only returned on TRAILING_STOP_MARKET order
	pub updateTime: i64,
	pub workingType: Option<String>, // no clue what this is
	pub priceProtect: bool,
	pub priceMatch: Option<String>, // huh
	pub selfTradePreventionMode: Option<String>,
	pub goodTillDate: Option<i64>,
}
impl FuturesPositionResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		// the way this works - is we sumbir "New" and "Query" to the same endpoint. The action is then determined by the presence of the orderId parameter.
		base_url.join("/fapi/v1/order").unwrap()
	}
}

#[derive(Serialize, Deserialize, Debug)]
struct FuturesBalance {
	accountAlias: String,
	asset: String,
	availableBalance: String,
	balance: String,
	crossUnPnl: String,
	crossWalletBalance: String,
	marginAvailable: bool,
	maxWithdrawAmount: String,
	updateTime: Number,
}

#[derive(Serialize, Deserialize, Debug)]
struct SpotAccountDetails {
	makerCommission: f64,
	takerCommission: f64,
	buyerCommission: f64,
	sellerCommission: f64,
	commissionRates: CommissionRates,
	canTrade: bool,
	canWithdraw: bool,
	canDeposit: bool,
	brokered: bool,
	requireSelfTradePrevention: bool,
	preventSor: bool,
	updateTime: u64,
	accountType: String,
	balances: Vec<SpotBalance>,
	permissions: Vec<String>,
	uid: u64,
}
#[derive(Serialize, Deserialize, Debug)]
struct CommissionRates {
	maker: String,
	taker: String,
	buyer: String,
	seller: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct SpotBalance {
	asset: String,
	free: String,
	locked: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct MarginAccountDetails {
	borrowEnabled: bool,
	marginLevel: String,
	CollateralMarginLevel: String,
	totalAssetOfBtc: String,
	totalLiabilityOfBtc: String,
	totalNetAssetOfBtc: String,
	TotalCollateralValueInUSDT: String,
	tradeEnabled: bool,
	transferEnabled: bool,
	accountType: String,
	userAssets: Vec<MarginUserAsset>,
}

#[derive(Serialize, Deserialize, Debug)]
struct MarginUserAsset {
	asset: String,
	borrowed: String,
	free: String,
	interest: String,
	locked: String,
	netAsset: String,
}

// FuturesExchangeInfo structs {{{
#[derive(Debug, Deserialize, Serialize)]
struct FuturesExchangeInfo {
	exchangeFilters: Vec<String>,
	rateLimits: Vec<RateLimit>,
	serverTime: i64,
	assets: Vec<Value>,
	symbols: Vec<FuturesSymbol>,
	timezone: String,
}
#[derive(Debug, Deserialize, Serialize)]
struct RateLimit {
	interval: String,
	intervalNum: u32,
	limit: u32,
	rateLimitType: String,
}

// the thing with multiplying orders due to weird limits should be here.
//#[derive(Debug, Deserialize, Serialize)]
//#[allow(non_snake_case)]
//struct SymbolFilter {
//	filterType: String,
//	maxPrice: String,
//	minPrice: String,
//	tickSize: String,
//	maxQty: String,
//	minQty: String,
//	stepSize: String,
//	limit: u32,
//	notional: String,
//	multiplierUp: String,
//	multiplierDown: String,
//	multiplierDecimal: u32,
//}

#[derive(Debug, Deserialize, Serialize)]
struct FuturesSymbol {
	symbol: String,
	pair: String,
	contractType: String,
	deliveryDate: i64,
	onboardDate: i64,
	status: String,
	maintMarginPercent: String,
	requiredMarginPercent: String,
	baseAsset: String,
	quoteAsset: String,
	marginAsset: String,
	pricePrecision: u32,
	quantityPrecision: usize,
	baseAssetPrecision: u32,
	quotePrecision: u32,
	underlyingType: String,
	underlyingSubType: Vec<String>,
	settlePlan: u32,
	triggerProtect: String,
	filters: Vec<Value>,
	OrderType: Option<Vec<String>>,
	timeInForce: Vec<String>,
	liquidationFee: String,
	marketTakeBound: String,
}

#[derive(Deserialize, Debug)]
struct FuturesAllPositionsResponse {
	entryPrice: String,
	breakEvenPrice: String,
	marginType: String,
	isAutoAddMargin: Value,
	isolatedMargin: String,
	leverage: String,
	liquidationPrice: String,
	markPrice: String,
	maxNotionalValue: String,
	positionAmt: String,
	notional: String,
	isolatedWallet: String,
	symbol: String,
	unRealizedProfit: String,
	positionSide: Value, // is "BOTH" in standard (non-hedge mode) requests, because designed by fucking morons. Apparently we now have negative values in `positionAmt`, if short.
	updateTime: i64,
}
impl FuturesAllPositionsResponse {
	pub fn get_url() -> Url {
		let base_url = Market::BinanceFutures.get_base_url();
		base_url.join("/fapi/v2/positionRisk").unwrap()
	}
}

#[derive(Serialize, Debug, Clone)]
pub struct FuturesOrder {
	pub symbol: String,
	pub price: f64,
	pub quantity: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseKline {
	open_time: i64,
	open: String,
	high: String,
	low: String,
	close: String,
	volume: String,
	close_time: u64,
	quote_asset_volume: String,
	number_of_trades: usize,
	taker_buy_base_asset_volume: String,
	taker_buy_quote_asset_volume: String,
	ignore: String,
}
//,}}}
