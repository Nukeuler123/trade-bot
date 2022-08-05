use crate::alpaca_api::{APIThreadReq, APIThreadRes};
use crate::market_strategies::{
    FibonacciRetracement, SingleMovingAverage, StockStrategy, StrategyOutput, SupportNResist,
    TwoMovingAverages,
};
use anyhow::{Error, Result};
use apca::data::v2::stream::Bar;
use chrono::{Datelike, Timelike, Utc};
use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::ops::Neg;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{error, info, warn};

pub struct StockMonitor {
    stock_strategy: Box<dyn StockStrategy + Send + Sync + 'static>,
    backtest_mode: bool,
    api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
    bought_stock: bool,
    emergency_margin_limit: f64, //If the price falls above or bellow this threshold relative to what the stock was bought at it will be sold, meant for sudden crashes
    bought_at: f64,
    pub symbol: String,
    buy_time: i32,
    upper_limit: Option<f64>,
    intensity: u32,
    how_much_bought: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SimplifiedDBMonitor {
    bought_stock: bool,
    buy_price: f64,
    strat_bytes: Vec<u8>,
    strat_name: String,
    buy_time: i32,
    how_much: u32,
}

impl StockMonitor {
    pub fn new(
        symbol: String,
        api_tx: Sender<(APIThreadReq, Sender<APIThreadRes>)>,
        backtest_mode: bool,
        strategy: String,
        emergency_margin_limit: f64,
        upper_limit: Option<f64>,
        intensity: u32,
    ) -> Self {
        //Select strat based on the config
        let strat: Box<dyn StockStrategy + Send + Sync> = match &*strategy {
            "Single Moving Average" => Box::new(SingleMovingAverage::new()),
            "Two Moving Averages" => Box::new(TwoMovingAverages::new()),
            "Support and Resist" => Box::new(SupportNResist::new()),
            "Fibonacci" => Box::new(FibonacciRetracement::new()),
            _ => {
                error!("[{}] Unknown strategy", &symbol);
                panic!("Unknown strategy set")
            }
        };

        if backtest_mode {
            info!("[{}] Starting in backtest mode", &symbol);
        }

        Self {
            stock_strategy: strat,
            backtest_mode,
            api_tx,
            bought_stock: false,
            emergency_margin_limit: emergency_margin_limit.neg(),
            bought_at: 0.0,
            symbol,
            buy_time: 0,
            upper_limit,
            intensity,
            how_much_bought: 0,
        }
    }

    pub fn run(&mut self, assets: Arc<RwLock<f64>>, bar_data: Option<Bar>) -> Result<()> {
        if self.backtest_mode {
            info!("[{}] Starting backtest", &self.symbol);
            self.run_backtest(assets)?;
            return Ok(());
        }

        //Should only be none if in backtest mode
        if bar_data.is_none() {
            return Err(Error::msg("No bar data provided and not in backtest mode!"));
        }

        let bar_data = bar_data.unwrap();

        //If we have not advanced one day since we bought, dont run. We need to swing trade
        if self.same_trade_buy_day() {
            return Ok(());
        }

        let strat_result = self.stock_strategy.run(&bar_data)?;

        let close: f64 = bar_data.close_price.to_f64().unwrap();

        //Create a return channel for when we make an API call
        let (res_tx, res_rx) = unbounded();
        if self.bought_stock {
            let percentage = ((close - self.bought_at) / self.bought_at) * 100.0;

            //check to see if price has dropped too much, if so sell
            if percentage <= self.emergency_margin_limit {
                self.api_tx
                    .send((
                        APIThreadReq::ApiSellStock {
                            symbol: self.symbol.clone(),
                            quantity: self.how_much_bought as usize,
                        },
                        res_tx,
                    ))
                    .unwrap();
                if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                    self.bought_stock = false;
                    warn!(
                        "[{}]: Emergency margin triggered!!! Sold at : {}",
                        &self.symbol, &close
                    );
                } else {
                    info!("[{}]: Error from alpaca API", &self.symbol);
                }
                return Ok(());
            }
            //Unwrap is fine here, the evaluation to see if it exists happens first, allowing the program to back out if the unwrap will be dangerous
            //Checks to see if we have hit the upper limit (set in config), if so, sell
            if self.upper_limit.is_some() && percentage >= self.upper_limit.unwrap() {
                self.api_tx
                    .send((
                        APIThreadReq::ApiSellStock {
                            symbol: self.symbol.clone(),
                            quantity: self.how_much_bought as usize,
                        },
                        res_tx,
                    ))
                    .unwrap();
                if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                    self.bought_stock = false;
                    warn!(
                        "[{}]: Upper bound triggered, Sold at : {}",
                        &self.symbol, &close,
                    );
                } else {
                    info!("[{}]: Error from alpaca API", &self.symbol);
                }
                return Ok(());
            }

            //It's friday, liquidate assets if it wont trigger PDT.
            if self.friday_near_end_of_trading_day() && !self.same_trade_buy_day() {
                info!("Nearing end of day friday, liquidating assets");
                self.sell(close, res_tx, res_rx);
                return Ok(());
            }
        }

        match strat_result {
            StrategyOutput::Buy => {
                self.buy(close, assets, res_tx, res_rx);
            }
            StrategyOutput::Sell => {
                self.sell(close, res_tx, res_rx);
            }
            StrategyOutput::Hold => {
                info!("[{}]: Holding...", &self.symbol);
            }
        }

        Ok(())
    }

    fn sell(
        &mut self,
        current_price: f64,
        res_tx: Sender<APIThreadRes>,
        res_rx: Receiver<APIThreadRes>,
    ) {
        if !self.bought_stock {
            info!("[{}]: Cannot sell, dont have stock", &self.symbol);
            return;
        }

        //Send sell request to API processing thread
        self.api_tx
            .send((
                APIThreadReq::ApiSellStock {
                    symbol: self.symbol.clone(),
                    quantity: self.how_much_bought as usize,
                },
                res_tx,
            ))
            .unwrap();

        //Make sure sell request is processed before updating stock state
        if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
            self.bought_stock = false;
            info!(
                "[{}]: sold {} shares at : {}",
                &self.symbol, self.how_much_bought, &current_price
            );
        } else {
            info!("[{}]: Error from alpaca API", &self.symbol);
        }
    }

    fn buy(
        &mut self,
        current_price: f64,
        usable_assets: Arc<RwLock<f64>>,
        res_tx: Sender<APIThreadRes>,
        res_rx: Receiver<APIThreadRes>,
    ) {
        if self.bought_stock {
            info!("[{}]: Cannot buy, already bought stock", &self.symbol);
            return;
        }
        let total_intensity: u32 = {
            //Calculate how many full shares we can buy
            let how_many_possible = (*usable_assets.read().unwrap() / current_price).floor() as u32;
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

        //Check to see if we can afford to buy
        if total_intensity == 0 {
            info!("[{}]: Cannot buy, not enough money available", &self.symbol);
            return;
        } else {
            //if so send buy request
            self.api_tx
                .send((
                    APIThreadReq::ApiBuyStock {
                        symbol: self.symbol.clone(),
                        quantity: total_intensity as usize,
                    },
                    res_tx,
                ))
                .unwrap();

            //Make sure buy request is accepted before updating stock state
            if let Ok(APIThreadRes::ApiProcessed) = res_rx.recv() {
                self.bought_at = current_price;
                self.bought_stock = true;
                self.buy_time = Utc::now().num_days_from_ce();
                self.how_much_bought = total_intensity;
                let total_calc: f64 = current_price * total_intensity as f64;
                info!(
                    "[{}]: Bought {} shares at : {} each, total of: {}",
                    &self.symbol, total_intensity, &self.bought_at, total_calc
                );
                info!("Stock Watcher suspended until next day");
            } else {
                info!("[{}]: Error from alpaca API", &self.symbol);
            }
        }
    }

    ///testing mode that uses files to run algorithms
    fn run_backtest(&mut self, assets: Arc<RwLock<f64>>) -> Result<()> {
        let mut money_made: f64 = 0.0;
        let mut reader =
            csv::Reader::from_path(format!("./backtest_data/{}.csv", &self.symbol)).unwrap();
        for record in reader.deserialize() {
            let (_, open, high, low, close, volume): (String, f64, f64, f64, f64, f64) =
                record.unwrap();

            let strat_result = self
                .stock_strategy
                .run_backtest(open, close, high, low, volume);
            //check to see if price has dropped too much
            if self.bought_stock {
                let percentage = ((close - self.bought_at) / self.bought_at) * 100.0;
                if percentage < self.emergency_margin_limit {
                    *assets.write().unwrap() += close;
                    money_made += close;
                    self.bought_stock = false;
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
                    if self.is_friday() {
                        info!("[{}]: Cannot buy, end of market week", &self.symbol);
                        continue;
                    }
                    let close: f64 = close;
                    let total_intensity: u32 = {
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
                        money_made -= close * total_intensity as f64;
                        *assets.write().unwrap() -= close * total_intensity as f64;
                        self.bought_at = close;
                        self.bought_stock = true;
                        self.how_much_bought = total_intensity;
                        let total_calc: f64 = close * total_intensity as f64;
                        info!(
                            "[{}]: Bought {} shares at : {} each, total of: {}",
                            &self.symbol, total_intensity, &self.bought_at, total_calc
                        );
                    }
                }
                StrategyOutput::Sell => {
                    if !self.bought_stock {
                        info!("[{}]: Cannot sell, dont have stock", &self.symbol);
                        continue;
                    }
                    money_made += close * self.how_much_bought as f64;
                    *assets.write().unwrap() += close * self.how_much_bought as f64;
                    self.bought_stock = false;
                    info!(
                        "[{}]: sold at at : {} with a total payout of {}",
                        &self.symbol,
                        &close,
                        close * self.how_much_bought as f64
                    );
                }
                StrategyOutput::Hold => {
                    //info!("[{}]: Holding...", &self.symbol);
                }
            }
        }
        info!("[{}] profit made: {}", &self.symbol, money_made);
        Ok(())
    }
    fn is_friday(&self) -> bool {
        let now = Utc::now();
        now.weekday().num_days_from_monday() >= 4
    }

    fn same_trade_buy_day(&self) -> bool {
        self.bought_stock && self.buy_time == Utc::now().num_days_from_ce()
    }

    fn friday_near_end_of_trading_day(&self) -> bool {
        let now = Utc::now();
        let is_friday: bool = now.weekday().num_days_from_monday() >= 4;
        let is_near_end: bool = now.hour() >= 18;

        is_friday && is_near_end
    }

    //Returns a struct that has the essential data for the monitor when the state is loaded
    pub fn save_state(&self) -> SimplifiedDBMonitor {
        let strat_data = &self.stock_strategy.save_state();
        SimplifiedDBMonitor {
            bought_stock: self.bought_stock,
            buy_price: self.bought_at,
            strat_bytes: strat_data.0.to_vec(),
            strat_name: strat_data.1.to_string(),
            buy_time: self.buy_time,
            how_much: self.how_much_bought,
        }
    }

    //Ran after the creation of a stock, sets the values in the monitor according to what's in the DB
    pub fn set_state(&mut self, simple_mon: SimplifiedDBMonitor) {
        self.bought_stock = simple_mon.bought_stock;
        self.bought_at = simple_mon.buy_price;
        let strat: Box<dyn StockStrategy + Send + Sync> = match &*simple_mon.strat_name {
            "Single Moving Average" => Box::new(
                bincode::deserialize::<SingleMovingAverage>(&simple_mon.strat_bytes).unwrap(),
            ),
            "Two Moving Averages" => Box::new(
                bincode::deserialize::<TwoMovingAverages>(&simple_mon.strat_bytes).unwrap(),
            ),
            "Support and Resist" => {
                Box::new(bincode::deserialize::<SupportNResist>(&simple_mon.strat_bytes).unwrap())
            }
            "Fibonacci" => Box::new(
                bincode::deserialize::<FibonacciRetracement>(&simple_mon.strat_bytes).unwrap(),
            ),
            _ => {
                error!("[{}] Unknown strategy", &self.symbol);
                panic!("Unknown strategy selected")
            }
        };
        //Self explanitor, if the current strategy and the one in the DB are the same, simply replace, else ignore the DB
        if self.stock_strategy.save_state().1 == simple_mon.strat_name {
            self.stock_strategy = strat;
        } else {
            info!("New strategy detected from config, ignoring old strategy in DB")
        }
        self.buy_time = simple_mon.buy_time;
        //To fix a minor error that happened before, leaving here just in cas
        if simple_mon.how_much == 0 && simple_mon.bought_stock {
            info!("Error detected, intensity is at 0 when bought stock is true, correcting");
            self.how_much_bought = 1;
        } else {
            self.how_much_bought = simple_mon.how_much;
        }
    }
}
