use anyhow::Error;
use apca::api::v2::order;
use apca::api::v2::order::OrderReqInit;
use apca::api::v2::order::Side::{Buy, Sell};
use apca::data::v2::stream::{drive, Data, MarketData, RealtimeData, IEX};
use apca::{ApiInfo, Client};
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::{FutureExt, StreamExt};
use num_decimal::Num;
use std::sync::{Arc, RwLock};
use std::thread::spawn;
use tracing::{error, info};

pub fn alpaca_api_thread(
    api_info: ApiInfo,
    assets: Arc<RwLock<f64>>,
    backtesting: bool,
    active_symbols: Vec<String>,
) -> (Sender<(APIThreadReq, Sender<APIThreadRes>)>, Receiver<Data>) {
    let (tx_req, rx_req) = unbounded();
    let (tx_data, rx_data) = unbounded();
    if backtesting {
        info!("In backtesting mode, alpaca API disabled");
        return (tx_req, rx_data);
    }
    spawn(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(3)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                info!("Alpaca order processing thread started! Listening for commands");
                let alpaca_client = Client::new(api_info.clone());
                let rx_req: Receiver<(APIThreadReq, Sender<APIThreadRes>)> = rx_req;
                let assets: Arc<RwLock<f64>> = assets;

                //This thread will listen for market data for our symbols and send them to the main thread for usage
                tokio::spawn(async move{
                    let tx_data: Sender<Data> = tx_data;
                    let alpaca_client = Client::new(api_info);

                    let (mut stream, mut subsription) = alpaca_client.subscribe::<RealtimeData<IEX>>().await.unwrap();

                    let mut data = MarketData::default();

                    info!("Watching symbols: {:#?}",&active_symbols);
                    data.set_bars(active_symbols);


                    let subscribe = subsription.subscribe(&data).boxed();

                    //"Drives" the websocket
                    let () = drive(subscribe, &mut stream)
                    .await.unwrap().unwrap().unwrap();
                    info!("Alpaca market data processing thread started! Forwarding bar data to main thread!");

                    loop {
                        //Waits for bar market data to come in, then sends it to the main processing thread
                        if let Some(Ok(Ok(market_data))) = stream.next().await {
                            if market_data.is_bar() {
                                tx_data.send(market_data).unwrap();
                            }
                        }
                    }
                });

                //Get the current cash from the alpaca account, must succeed
                let acct_data = alpaca_client
                    .issue::<apca::api::v2::account::Get>(&())
                    .await
                    .unwrap();
                info!(
                    "Alpaca API reports: ${} in account, loading into memory",
                    acct_data.cash.to_f64().unwrap()
                );
                *assets.write().unwrap() = acct_data.cash.to_f64().unwrap();
                drop(acct_data);

                for rx in rx_req.iter() {
                    //Create order
                    let req_init: OrderReqInit = OrderReqInit {
                        type_: order::Type::Market,
                        ..Default::default()
                    };

                    //make order buy or sell
                    let req = match rx.0 {
                        APIThreadReq::ApiBuyStock { symbol, quantity } => {
                            info!(
                                "Processing API buy call for symbol: {} of quantity: {}",
                                &symbol, quantity
                            );
                            req_init.init(symbol, Buy, order::Amount::quantity(quantity))
                        }
                        APIThreadReq::ApiSellStock { symbol, quantity } => {
                            info!(
                                "Processing API sell call for symbol: {} of quantity: {}",
                                &symbol, quantity
                            );
                            req_init.init(symbol, Sell, order::Amount::quantity(quantity))
                        }
                        APIThreadReq::ApiBuyCrypto { symbol, quantity } => {
                            info!(
                                "Processing Crypto API buy call for symbol: {} of fraction value: {}",
                                &symbol, quantity
                            );
                            req_init.init(symbol, Buy, order::Amount::quantity(quantity))
                        }
                        APIThreadReq::ApiSellCrypto { symbol, quantity } => {
                            info!(
                                "Processing Crypto API sell call for symbol: {} of fraction value: {}",
                                &symbol, quantity
                            );
                            req_init.init(symbol, Sell, order::Amount::quantity(quantity))
                        }
                    };

                    //Return result
                    match alpaca_client.issue::<order::Post>(&req).await {
                        Ok(_) => {
                            rx.1.send(APIThreadRes::ApiProcessed).unwrap();
                            info!("Processesed API call");
                        }
                        Err(e) => {
                            error!("API Error: {:#?}", e);
                            rx.1.send(APIThreadRes::ApiError { error: e.into() })
                                .unwrap();
                        }
                    }

                    //Instead of roughly calculating the money simply pull the data from the broker directly and update the amount of fiat assets we have
                    match alpaca_client
                        .issue::<apca::api::v2::account::Get>(&())
                        .await
                    {
                        Ok(acct_data) => {
                            if let Some(cash) = acct_data.cash.to_f64() {
                                *assets.write().unwrap() = cash;
                            }
                        }
                        Err(e) => {
                            error!("API Error could not update asset_data: {:#?}", e);
                        }
                    }
                }
                info!("All senders dropped! Exiting API thread!")
            })
    });
    (tx_req, rx_data)
}

pub enum APIThreadReq {
    ApiBuyStock { symbol: String, quantity: usize },
    ApiSellStock { symbol: String, quantity: usize },
    ApiBuyCrypto { symbol: String, quantity: Num },
    ApiSellCrypto { symbol: String, quantity: Num },
}

pub enum APIThreadRes {
    ApiProcessed,
    ApiError { error: Error },
}
