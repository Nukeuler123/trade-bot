use chrono::{Datelike, Timelike, Utc};
use crossbeam_channel::unbounded;
use std::sync::{Arc, RwLock};
use std::thread::sleep;
use std::thread::spawn;
use std::time::Duration;

use crate::alpaca_api::APIThreadRes;
use crate::crypto_processing::crypto_monitor::CryptoMonitor;
use crate::StockMonitor;
use anyhow::Error;
use sled::Db;
use tracing::{error, info};

pub fn start_loop(
    backtesting: bool,
    crypto: Vec<Arc<RwLock<CryptoMonitor>>>,
    allocated_currency: Arc<RwLock<f64>>,
    interval: u32,
    db: Arc<Db>,
) {
    if backtesting {
        //backtest_loop(crypto, allocated_currency);
    } else {
        start_loop_normal(crypto, allocated_currency, interval, db);
    }
}

//This is lazy but necessary as the two loops work on different time scales
fn start_loop_normal(
    cryptos: Vec<Arc<RwLock<CryptoMonitor>>>,
    allocated_currency: Arc<RwLock<f64>>,
    interval: u32,
    state_db: Arc<Db>,
) {
    let allocated_currency: Arc<RwLock<f64>> = allocated_currency;
    info!("Ticker(Crypto) loop started!");

    loop {
        //If the program is not on interval, wait
        if !time_check(interval) {
            sleep(Duration::from_millis(20));
            continue;
        }

        let last_money_value: f64 = *allocated_currency.clone().read().unwrap();
        info!("Processing crypto...");
        //Process all crypto and pass it the data required to run correctly
        for crypto in &cryptos {
            let crypto = crypto.clone();
            let assets = allocated_currency.clone();
            let db = state_db.clone();
            spawn(move || {
                let crypto = crypto;
                let assets = assets;
                let db = db;

                match crypto.write() {
                    Ok(mut crypto_wrt) => {
                        match crypto_wrt.run(assets) {
                            Ok(_) => {
                                info!("Saving monitor state for symbol: {}", &crypto_wrt.symbol);
                                //Save the state of the stock to the local DB
                                let state = crypto_wrt.save_state();
                                let _ = db.insert(
                                    crypto_wrt.symbol.as_bytes(),
                                    bincode::serialize(&state).unwrap(),
                                );
                            }
                            Err(e) => {
                                error!("[{}] Error: {:#?}", crypto_wrt.symbol, e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("RWLOCK error: {:#?}", e);
                    }
                };
            });
        }
        let profit = *allocated_currency.clone().read().unwrap() - last_money_value;
        info!("Profit made: {}", profit);
        sleep(Duration::from_secs(60));
    }
}
/*
fn backtest_loop(stocks: Vec<Arc<RwLock<StockMonitor>>>, allocated_currency: Arc<RwLock<f64>>) {
    let allocated_currency: Arc<RwLock<f64>> = allocated_currency.clone();
    let last_money_value: f64 = *allocated_currency.clone().read().unwrap();
    info!("Processing stocks...");
    for stock in &stocks {
        let stock = stock.clone();
        let assets = allocated_currency.clone();
        spawn(move || {
            let stock = stock;
            let assets = assets;

            match stock.write() {
                Ok(mut stock_wrt) => match stock_wrt.run(assets) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("[{}] Error: {:#?}", stock_wrt.symbol, e);
                    }
                },
                Err(e) => {
                    error!("RWLOCK error: {:#?}", e);
                }
            };
        });
    }
    sleep(Duration::from_secs(2));
    info!("Done! Looking for errors from threads");
    let profit = *allocated_currency.clone().read().unwrap() - last_money_value;
    info!("Ending currency: {}", allocated_currency.read().unwrap());
    info!("Profit made: {}", profit);
}

 */

//Removed need for market hours as this is crypto, market hours is 24/7
fn time_check(min_inteval: u32) -> bool {
    let now = Utc::now();
    let on_interval: bool = now.minute() % min_inteval == 0;
    on_interval
}
