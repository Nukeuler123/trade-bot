use crate::StockMonitor;
use apca::data::v2::stream::{Bar, Data};
use chrono::{Datelike, Utc};
use crossbeam_channel::Receiver;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread::sleep;
use std::thread::spawn;
use std::time::Duration;
use threadpool::ThreadPool;

use sled::Db;
use tracing::{error, info};

pub fn start_loop(
    backtesting: bool,
    stocks: HashMap<String, Arc<RwLock<StockMonitor>>>,
    allocated_currency: Arc<RwLock<f64>>,
    bar_data: Receiver<Data>,
    db: Arc<Db>,
    threadpool: ThreadPool,
) {
    if backtesting {
        backtest_loop(stocks, allocated_currency);
    } else {
        start_loop_normal(stocks, allocated_currency, bar_data, db, threadpool);
    }
}

///The main meat of the code, this handles the creation of threads for each stock monitor
fn start_loop_normal(
    stocks: HashMap<String, Arc<RwLock<StockMonitor>>>,
    allocated_currency: Arc<RwLock<f64>>,
    bar_data: Receiver<Data>,
    state_db: Arc<Db>,
    threadpool: ThreadPool,
) {
    let allocated_currency: Arc<RwLock<f64>> = allocated_currency;
    info!("Ticker(Stock) loop started!");

    loop {
        //If the program is not in market day (Its the weekend), wait
        if !time_check() {
            sleep(Duration::from_millis(2000));
            continue;
        }

        if !bar_data.is_empty() {
            info!("Processing stocks...");
        } else {
            sleep(Duration::from_millis(500));
            continue;
        }

        let last_money_value: f64 = *allocated_currency.clone().read().unwrap();

        let mut created = 0;
        //try and get the newest stock data from the alpaca market data processor
        for data in bar_data.try_recv() {
            if let Data::Bar(bar) = data {
                //Wait about a second per 4 stock monitors, dont want to cause rate limits
                if created >= 4 {
                    sleep(Duration::from_millis(1100));
                    created = 0;
                }

                //Clone all the arcs to they can be explicitly moved with no fuss
                let stock: Arc<RwLock<StockMonitor>> = stocks.get(&bar.symbol).unwrap().clone();
                let assets: Arc<RwLock<f64>> = allocated_currency.clone();
                let db: Arc<Db> = state_db.clone();

                threadpool.execute(move || {
                    //Explicit move
                    let stock: Arc<RwLock<StockMonitor>> = stock;
                    let assets: Arc<RwLock<f64>> = assets;
                    let db: Arc<Db> = db;
                    let bar_data: Bar = bar;

                    //get write access to stock monitor, should NEVER error because there shouldn't be any panics in this part of the code
                    match stock.write() {
                        Ok(mut stock_wrt) => {
                            match stock_wrt.run(assets, Some(bar_data)) {
                                Ok(_) => {
                                    info!("Saving stock state for symbol: {}", &stock_wrt.symbol);
                                    //Save the state of the stock to the local stock state DB
                                    let state = stock_wrt.save_state();
                                    let _ = db.insert(
                                        stock_wrt.symbol.as_bytes(),
                                        bincode::serialize(&state).unwrap(),
                                    );
                                }
                                Err(e) => {
                                    error!("[{}] Error: {:#?}", stock_wrt.symbol, e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("RWLOCK error: {:#?}", e);
                        }
                    };
                });

                created += 1;
            }
        }

        let profit = *allocated_currency.clone().read().unwrap() - last_money_value;
        info!("Profit made: {}", profit);
        sleep(Duration::from_millis(500));
    }
}

fn backtest_loop(
    stocks: HashMap<String, Arc<RwLock<StockMonitor>>>,
    allocated_currency: Arc<RwLock<f64>>,
) {
    let allocated_currency: Arc<RwLock<f64>> = allocated_currency.clone();
    let last_money_value: f64 = *allocated_currency.clone().read().unwrap();
    info!("Processing stocks...");

    //Runs all the stock monitors in backtest mode, data will be grabbed from a directory called "backtest_data", with the corresponding symbol being pulled from disk
    //IE: AAPL.csv
    for stock in stocks {
        let stock = stock.clone();
        let assets = allocated_currency.clone();
        spawn(move || {
            let stock = stock.1.clone();
            let assets = assets;

            match stock.write() {
                Ok(mut stock_wrt) => match stock_wrt.run(assets, None) {
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

fn time_check() -> bool {
    let now = Utc::now();
    let in_market_hours: bool = now.weekday().num_days_from_monday() <= 4;
    //if we're in the market hours and on interval, return true, else false
    in_market_hours
}
