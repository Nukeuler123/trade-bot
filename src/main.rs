extern crate core;

mod alpaca_api;
mod config;
//mod crypto_processing;
mod market_strategies;
mod stock_processing;

use crate::config::BotConfig;
use anyhow::Result;
use apca::ApiInfo;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::{panic, process};
use threadpool::ThreadPool;
//use std::thread::spawn;

use crate::alpaca_api::alpaca_api_thread;
//use crate::crypto_processing::crypto_monitor::{CryptoMonitor, SimplifiedCryptoDBMonitor};
use crate::stock_processing::stock_monitor::{SimplifiedDBMonitor, StockMonitor};
use tracing::{info, Level};

fn logger_init() {
    tracing_subscriber::fmt()
        .compact()
        .with_thread_names(true)
        .with_max_level(Level::INFO)
        .init();
    info!("Logger initialized")
}

fn main() -> Result<()> {
    logger_init();

    let config: BotConfig = BotConfig::load_config();
    info!("Loading state DB");

    //Loads the stock monitors from config, using DB to set their last state (if they bought stocks and such)
    let stock_state_db = Arc::new(sled::open("./stock_state").unwrap());

    //Set allocated currency to zero and then start up the alpaca API thread which will set the current buying power/cash as the allocated currency
    let allowed_currency: Arc<RwLock<f64>> = Arc::new(RwLock::new(0.0));
    let api_info = ApiInfo::from_parts(
        config.keys.alpaca_base_url,
        config.keys.alpaca_key_id,
        config.keys.alpaca_key_secret,
    )
    .unwrap();

    //When starting the API thread pass what stock symbols will be used
    let active_stocks: Vec<String> = config
        .stocks
        .iter()
        .map(|stock| stock.get_symbol())
        .collect();

    let (tx, rx) = alpaca_api_thread(
        api_info,
        allowed_currency.clone(),
        config.testing_mode,
        active_stocks,
    );

    //Load stocks from config and load any of their past states from the DB
    let backtesting = config.testing_mode;
    let mut stock_monitors_safe: HashMap<String, Arc<RwLock<StockMonitor>>> = HashMap::default();
    for stock in config.stocks {
        let name = stock.get_symbol();
        let mut stock_monitor = stock.convert(backtesting, tx.clone());

        //If the stock's name is in the DB load the old state
        if let Ok(Some(data)) = stock_state_db.get(stock_monitor.symbol.as_bytes()) {
            info!(
                "Loading past stock state for symbol: {}",
                &stock_monitor.symbol
            );
            let raw_bytes = data.to_vec();

            let simplified_data: SimplifiedDBMonitor = bincode::deserialize(&raw_bytes).unwrap();

            stock_monitor.set_state(simplified_data);
        }

        stock_monitors_safe.insert(name, Arc::new(RwLock::new(stock_monitor)));
    }
    /*
    let mut crypto_monitors_safe: Vec<Arc<RwLock<CryptoMonitor>>> = vec![];
    for crypto in config.crypto {
        let mut crypto_monitor = crypto.convert(backtesting, tx.clone());

        if let Ok(Some(data)) = stock_state_db.get(crypto_monitor.symbol.as_bytes()) {
            info!(
                "Loading past stock state for symbol: {}",
                &crypto_monitor.symbol
            );
            let raw_bytes = data.to_vec();

            let simplified_data: SimplifiedCryptoDBMonitor =
                bincode::deserialize(&raw_bytes).unwrap();

            crypto_monitor.set_state(simplified_data);
        }

        crypto_monitors_safe.push(Arc::new(RwLock::new(crypto_monitor)));
    }

    //TODO make crypto trading worthwhile
    //let tmp_currency_clone = allowed_currency.clone();
    //let tmp_db_clone = stock_state_db.clone();
    spawn(move || {
        crypto_processing::crypto_ticker_loop::start_loop(
            backtesting,
            crypto_monitors_safe,
            tmp_currency_clone,
            config.crypto_engine_config.tick_interval,
            tmp_db_clone
        )
    });
     */

    if backtesting {
        *allowed_currency.write().unwrap() = config.stock_engine_config.backtest_money;
    }

    //Incase the alpaca data processing thread crashes simply poison pill (terminate) the entire program
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(-1);
    }));

    let pool = ThreadPool::new(config.stock_engine_config.threads);

    //Begin running stock loop
    stock_processing::stock_ticker_loop::start_loop(
        backtesting,
        stock_monitors_safe,
        allowed_currency,
        rx,
        stock_state_db,
        pool,
    );

    Ok(())
}
