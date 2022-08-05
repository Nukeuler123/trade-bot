use crate::StockMonitor;

use crate::alpaca_api::{APIThreadReq, APIThreadRes};
//use crate::crypto_processing::crypto_monitor::CryptoMonitor;
use crossbeam_channel::Sender;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use tracing::info;

#[derive(Deserialize)]
pub struct BotConfig {
    pub keys: ApiKeys,
    pub stocks: Vec<Stock>,
    //pub crypto: Vec<Crypto>,
    pub stock_engine_config: EngineConfig,
    //pub crypto_engine_config: EngineConfig,
    pub testing_mode: bool,
}

#[derive(Deserialize)]
pub struct EngineConfig {
    pub backtest_money: f64,
    pub threads: usize,
}

/*
#[derive(Deserialize)]
pub struct Crypto {
    symbol: String,
    strategy: String,
    emergency_limit: f64,
    upper_limit: Option<f64>,
    buy_max_dollar_value: u32,
}

impl Crypto {
    pub fn convert(
        self,
        backtest_mode: bool,
        api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
    ) -> CryptoMonitor {
        CryptoMonitor::new(
            self.symbol,
            api_tx,
            backtest_mode,
            self.strategy,
            self.emergency_limit,
            self.upper_limit,
            self.buy_max_dollar_value,
        )
    }
}
*/

#[derive(Deserialize)]
pub struct Stock {
    symbol: String,
    strategy: String,
    emergency_limit: f64,
    upper_limit: Option<f64>,
    intensity: u32,
}
impl Stock {
    pub fn get_symbol(&self) -> String {
        self.symbol.clone()
    }

    //Convert a stock in the config into a monitor
    pub fn convert(
        self,
        backtest_mode: bool,
        api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
    ) -> StockMonitor {
        StockMonitor::new(
            self.symbol,
            api_tx,
            backtest_mode,
            self.strategy,
            self.emergency_limit,
            self.upper_limit,
            self.intensity,
        )
    }
}

#[derive(Deserialize)]
pub struct ApiKeys {
    pub alpaca_key_id: String,
    pub alpaca_key_secret: String,
    pub alpaca_base_url: String,
}

impl BotConfig {
    pub fn load_config() -> Self {
        let mut file = File::open("./Config.toml").unwrap();
        let mut buffer: String = String::new();
        file.read_to_string(&mut buffer).unwrap();

        info!("Loaded config");
        toml::from_str(&buffer).unwrap()
    }
}
