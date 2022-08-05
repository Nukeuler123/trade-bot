use crate::alpaca_api::{APIThreadReq, APIThreadRes};
use crate::data_input::{CryptoDataInput, StockDataInput};
use crate::json_structs::{CryptoMarketData, MarketData};
use crate::market_strategies::{
    CryptoStrategy, SingleMovingAverage, StrategyOutput, TwoMovingAverages,
};
use anyhow::Result;
use apca::api::v2::order::Amount;
use chrono::{Datelike, Utc};
use crossbeam_channel::{unbounded, RecvError, Sender};
use num_decimal::Num;
use serde::{Deserialize, Serialize};
use std::ops::Neg;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{error, info, warn};

pub struct CryptoMonitor {
    crypto_strategy: Box<dyn CryptoStrategy + Send + Sync + 'static>,
    backtest_mode: bool,
    api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
    input: CryptoDataInput,
    bought_crypto: bool,
    emergency_margin_limit: f64, //If the price falls above or bellow this threshold relative to what the stock was bought at it will be sold, meant for sudden crashes
    bought_at: f64,
    pub symbol: String,
    upper_limit: Option<f64>,
    buy_limit: u32,
    how_much_bought: Num,
}

#[derive(Serialize, Deserialize)]
pub struct SimplifiedCryptoDBMonitor {
    bought_crypto: bool,
    buy_price: f64,
    strat_bytes: Vec<u8>,
    strat_name: String,
    how_much: Num,
}

impl CryptoMonitor {
    pub fn new(
        symbol: String,
        api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
        backtest_mode: bool,
        strategy: String,
        emergency_margin_limit: f64,
        upper_limit: Option<f64>,
        buy_limit: u32,
    ) -> Self {
        let strat: Box<dyn CryptoStrategy + Send + Sync> = match &*strategy {
            "Single Moving Average" => Box::new(SingleMovingAverage::new()),
            "Two Moving Averages" => Box::new(TwoMovingAverages::new()),
            _ => {
                error!("[{}] Unknown strategy", &symbol);
                panic!("Unknown strategy selected")
            }
        };

        if backtest_mode {
            info!("[{}] Starting in backtest mode", &symbol);
        }
        let id = {
            match symbol.to_lowercase().as_str() {
                "btcusd" => "bitcoin".to_string(),
                "ethusd" => "ethereum".to_string(),
                &_ => {
                    error!("[{}] Unknown coin", &symbol);
                    panic!("unknown coin: {}", &symbol);
                }
            }
        };

        let input = CryptoDataInput::new(format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
            &id
        ));

        Self {
            crypto_strategy: strat,
            backtest_mode,
            api_tx,
            input,
            bought_crypto: false,
            emergency_margin_limit: emergency_margin_limit.neg(),
            bought_at: 0.0,
            symbol,
            upper_limit,
            buy_limit,
            how_much_bought: Num::default(),
        }
    }

    pub fn run(&mut self, assets: Arc<RwLock<f64>>) -> Result<()> {
        if self.backtest_mode {
            info!("[{}] Starting backtest", &self.symbol);
            //
            // self.run_backtest(assets)?;
            return Ok(());
        }

        let data: CryptoMarketData = self.input.get_data()?;

        let strat_result = self.crypto_strategy.run(&data);

        let (res_tx, res_rx) = unbounded();
        //check to see if price has dropped too much, if so sell
        if self.bought_crypto {
            let percentage = (((data.usd) - self.bought_at) / self.bought_at) * 100.0;
            if percentage <= self.emergency_margin_limit {
                self.api_tx
                    .send((
                        APIThreadReq::ApiSellCrypto {
                            symbol: self.symbol.clone(),
                            quantity: self.how_much_bought.clone(),
                        },
                        res_tx,
                    ))
                    .unwrap();
                if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                    self.bought_crypto = false;
                    warn!(
                        "[{}]: Emergency margin triggered!!! Sold at : {}",
                        &self.symbol, &data.usd
                    );
                } else {
                    info!("[{}]: Error from alpaca API", &self.symbol);
                }
                return Ok(());
            }
            //Unwrap is fine here, the evaluation to see if it exists happens first, allowing the program to back out if the unwrap will be dangerous
            if self.upper_limit.is_some() && percentage >= self.upper_limit.unwrap() {
                self.api_tx
                    .send((
                        APIThreadReq::ApiSellCrypto {
                            symbol: self.symbol.clone(),
                            quantity: self.how_much_bought.clone(),
                        },
                        res_tx,
                    ))
                    .unwrap();
                if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                    self.bought_crypto = false;
                    warn!(
                        "[{}]: Upper bound triggered, Sold at : {}",
                        &self.symbol, &data.usd
                    );
                } else {
                    info!("[{}]: Error from alpaca API", &self.symbol);
                }
                return Ok(());
            }
        }

        match strat_result {
            StrategyOutput::Buy => {
                if self.bought_crypto {
                    info!("[{}]: Cannot buy, already bought stock", &self.symbol);
                    return Ok(());
                }
                let total_money_to_use: u32 = {
                    //If we can afford the buy limit use the buy limit, else use how much money we do have
                    if *assets.read().unwrap() as u32 > self.buy_limit {
                        self.buy_limit + 1
                    } else {
                        *assets.read().unwrap() as u32 + 1
                    }
                };

                //Check to see if we can afford to buy the minimal ammounts (1$)
                if total_money_to_use <= 1 {
                    info!("[{}]: Cannot buy, not enough money available", &self.symbol);
                    return Ok(());
                } else {
                    info!("Money to use: {}", total_money_to_use);
                    //if so, buy
                    self.api_tx
                        .send((
                            APIThreadReq::ApiBuyCrypto {
                                symbol: self.symbol.clone(),
                                quantity: Num::new(total_money_to_use, data.usd as u32),
                            },
                            res_tx,
                        ))
                        .unwrap();
                    if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                        self.bought_at = data.usd;
                        self.bought_crypto = true;
                        //Use num-decimal crate to turn our buy money into a crypto fraction
                        self.how_much_bought = Num::new(total_money_to_use, data.usd as u32);
                        info!(
                            "[{}]: Bought {} USD worth of crypto at {} per 1.0 fraction",
                            &self.symbol, total_money_to_use, &self.bought_at
                        );
                    } else {
                        info!("[{}]: Error from alpaca API", &self.symbol);
                    }
                }
            }
            StrategyOutput::Sell => {
                if !self.bought_crypto {
                    info!("[{}]: Cannot sell, dont have crypto", &self.symbol);
                    return Ok(());
                }
                if (((data.usd) - self.bought_at) / self.bought_at) * 100.0 < 2.0 {
                    info!(
                        "[{}]: Cannot sell crypto, fee outweighs profits",
                        &self.symbol
                    );
                    return Ok(());
                }
                //Sell
                self.api_tx
                    .send((
                        APIThreadReq::ApiSellCrypto {
                            symbol: self.symbol.clone(),
                            quantity: self.how_much_bought.clone(),
                        },
                        res_tx,
                    ))
                    .unwrap();
                if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                    self.bought_crypto = false;
                    info!(
                        "[{}]: sold {} dollars worth of crypto at {} per 1.0 fraction",
                        &self.symbol, self.how_much_bought, &data.usd
                    );
                } else {
                    info!("[{}]: Error from alpaca API", &self.symbol);
                }
            }
            StrategyOutput::Hold => {
                info!("[{}]: Holding...", &self.symbol);
            }
        }

        Ok(())
    }

    /*
    ///testing mode that uses files to run algorithms
    fn run_backtest(&mut self, assets: Arc<RwLock<f64>>) -> Result<()> {
        let mut money_made: f64 = 0.0;
        let mut reader =
            csv::Reader::from_path(format!("./backtest_data/{}.csv", &self.symbol)).unwrap();
        let mut wait_lines: usize = 0;
        for record in reader.deserialize() {
            let (date, open, high, low, close, volume): (String, f64, f64, f64, f64, f64) =
                record.unwrap();

            let data = MarketData {
                open: Some(open),
                high: Some(high),
                low: Some(low),
                last: None,
                close: Some(close),
                volume: Some(volume),
                date,
                symbol: "".to_string(),
                exchange: "".to_string(),
            };

            let strat_result = self.stock_strategy.run(&data)?;

            if self.bought_crypto && wait_lines < 25 {
                wait_lines +=1;
                continue;
            }
            //check to see if price has dropped too much
            if self.bought_crypto {
                let percentage = ((data.close.unwrap() - self.bought_at) / self.bought_at) * 100.0;
                if percentage < self.emergency_margin_limit {
                    *assets.write().unwrap() += data.close.unwrap();
                    money_made += data.close.unwrap();
                    self.bought_crypto = false;
                    //warn!(
                    //"[{}]: Emergency margin triggered!!! Sold at : {}",
                    //&self.symbol,
                    // &data.close.unwrap()
                    //);
                    continue;
                }
            }

            match strat_result {
                StrategyOutput::Buy => {
                    if self.bought_stock {
                        info!("[{}]: Cannot buy, already bought stock", &self.symbol);
                        continue;
                    }
                    let close: f64 = data.close.unwrap();
                    let total_intensity: u32 ={
                        //Calculate how many full shares we can buy
                        let how_many_possible = (*assets.read().unwrap() / close).floor() as u32;
                        //If the allocated amount of shares is less than or equal to max possible (IE we're allowed to buy 5 but have the ability to buy 10)
                        //Just return the set number
                        if self.intensity <= how_many_possible {
                            self.intensity
                        }
                        //Else, we cant buy the allocated amount shares, just buy as much as we can
                        else {
                            how_many_possible
                        }
                    };

                    if total_intensity == 0 {
                        //  info!("[{}]: Cannot buy, not enough money available", &self.symbol);
                        continue;
                    } else {
                        money_made -= data.close.unwrap();
                        *assets.write().unwrap() -= data.close.unwrap();
                        self.bought_at = data.close.unwrap();
                        self.bought_stock = true;
                        let total_calc: f64 = data.close.unwrap() * total_intensity as f64;
                        info!("[{}]: Bought {} shares at : {} each, total of: {}", &self.symbol, total_intensity,&self.bought_at, total_calc);
                    }
                }
                StrategyOutput::Sell => {
                    if !self.bought_stock {
                        info!("[{}]: Cannot sell, dont have stock", &self.symbol);
                        continue;
                    }
                    money_made += data.close.unwrap() * self.how_much_bought as f64;
                    *assets.write().unwrap() += data.close.unwrap() * self.how_much_bought as f64;
                    self.bought_stock = false;
                    info!("[{}]: sold at at : {}", &self.symbol, &data.close.unwrap());
                }
                StrategyOutput::Hold => {
                    info!("[{}]: Holding...", &self.symbol);
                }
            }
        }
        info!("[{}] profit made: {}", &self.symbol, money_made);
        Ok(())
    }

     */
    pub fn save_state(&self) -> SimplifiedCryptoDBMonitor {
        let strat_data = &self.crypto_strategy.save_state();
        SimplifiedCryptoDBMonitor {
            bought_crypto: self.bought_crypto,
            buy_price: self.bought_at,
            strat_bytes: strat_data.0.to_vec(),
            strat_name: strat_data.1.to_string(),
            how_much: self.how_much_bought.clone(),
        }
    }

    pub fn set_state(&mut self, simple_mon: SimplifiedCryptoDBMonitor) {
        self.bought_crypto = simple_mon.bought_crypto;
        self.bought_at = simple_mon.buy_price;
        let strat: Box<dyn CryptoStrategy + Send + Sync> = match &*simple_mon.strat_name {
            "Single Moving Average" => Box::new(
                bincode::deserialize::<SingleMovingAverage>(&simple_mon.strat_bytes).unwrap(),
            ),
            "Two Moving Averages" => Box::new(
                bincode::deserialize::<TwoMovingAverages>(&simple_mon.strat_bytes).unwrap(),
            ),
            _ => {
                error!("[{}] Unknown strategy", &self.symbol);
                panic!("Unknown strategy selected")
            }
        };
        self.crypto_strategy = strat;
        self.how_much_bought = simple_mon.how_much;
    }
}
