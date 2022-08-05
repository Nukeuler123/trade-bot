use anyhow::{Error, Ok};
use apca::data::v2::stream::Bar;
use serde::{Deserialize, Serialize};
use ta::indicators::ExponentialMovingAverage;
use ta::Next;
use tracing::info;

/*
pub trait CryptoStrategy {
    fn run(&mut self, data: &Bar) -> StrategyOutput;
    fn save_state(&self) -> (Vec<u8>, String);
}
*/

//This trait is the base for all Strategies, if you want to implement one, make sure your struct implements this
pub trait StockStrategy {
    fn run_backtest(
        &mut self,
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
    ) -> StrategyOutput;
    fn run(&mut self, data: &Bar) -> anyhow::Result<StrategyOutput>;
    fn save_state(&self) -> (Vec<u8>, String);
}

pub enum StrategyOutput {
    Buy,
    Sell,
    Hold,
}

#[derive(Serialize, Deserialize)]
pub struct SingleMovingAverage {
    ema: ExponentialMovingAverage, //The core math formula
}

impl SingleMovingAverage {
    pub fn new() -> Self {
        Self {
            ema: ExponentialMovingAverage::new(2).unwrap(),
        }
    }
}

/*
impl CryptoStrategy for SingleMovingAverage {
    fn run(&mut self, data: &Bar) -> StrategyOutput {
        //get the new average
        let avg = self.ema.next(data);

        if data.usd > avg {
            return StrategyOutput::Buy;
        }
        if data.usd < avg {
            return StrategyOutput::Sell;
        }
        StrategyOutput::Hold
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (
            bincode::serialize(&self).unwrap(),
            "Single Moving Average".to_string(),
        )
    }
}
*/

impl StockStrategy for SingleMovingAverage {
    fn run(&mut self, data: &Bar) -> anyhow::Result<StrategyOutput> {
        //get the new average
        let close = data.close_price.to_f64().unwrap();
        let avg = self.ema.next(close);

        if close > avg {
            return Ok(StrategyOutput::Buy);
        }
        if close < avg {
            return Ok(StrategyOutput::Sell);
        }
        Ok(StrategyOutput::Hold)
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (
            bincode::serialize(&self).unwrap(),
            "Single Moving Average".to_string(),
        )
    }

    fn run_backtest(
        &mut self,
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
    ) -> StrategyOutput {
        //get the new average
        let avg = self.ema.next(close);

        if close > avg {
            return StrategyOutput::Buy;
        }
        if close < avg {
            return StrategyOutput::Sell;
        }
        StrategyOutput::Hold
    }
}

#[derive(Serialize, Deserialize)]
pub struct TwoMovingAverages {
    ema_one: ExponentialMovingAverage,
    ema_two: ExponentialMovingAverage,
}

impl TwoMovingAverages {
    pub fn new() -> Self {
        Self {
            ema_one: ExponentialMovingAverage::new(2).unwrap(),
            ema_two: ExponentialMovingAverage::new(6).unwrap(),
        }
    }
}

/*
impl CryptoStrategy for TwoMovingAverages {
    fn run(&mut self, data: &CryptoMarketData) -> StrategyOutput {
        let avg_one = self.ema_one.next(data.usd);
        let avg_two = self.ema_two.next(data.usd);

        if avg_one > avg_two {
            return StrategyOutput::Buy;
        }
        if avg_one < avg_two {
            return StrategyOutput::Sell;
        }
        StrategyOutput::Hold
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (
            bincode::serialize(&self).unwrap(),
            "Two Moving Averages".to_string(),
        )
    }
}
*/

impl StockStrategy for TwoMovingAverages {
    fn run(&mut self, data: &Bar) -> anyhow::Result<StrategyOutput> {
        //get the new average
        let close = data.close_price.to_f64().unwrap();
        let avg_one = self.ema_one.next(close);
        let avg_two = self.ema_two.next(close);

        if avg_one > avg_two {
            return Ok(StrategyOutput::Buy);
        }
        if avg_one < avg_two {
            return Ok(StrategyOutput::Sell);
        }
        Ok(StrategyOutput::Hold)
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (
            bincode::serialize(&self).unwrap(),
            "Two Moving Averages".to_string(),
        )
    }

    fn run_backtest(
        &mut self,
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
    ) -> StrategyOutput {
        //get the new average

        let avg_one = self.ema_one.next(close);
        let avg_two = self.ema_two.next(close);

        if avg_one > avg_two {
            return StrategyOutput::Buy;
        }
        if avg_one < avg_two {
            return StrategyOutput::Sell;
        }
        StrategyOutput::Hold
    }
}

#[derive(Serialize, Deserialize)]
pub struct SupportNResist {
    past_high: f64,
    past_low: f64,
    past_close: f64,
    ran_before: bool,
}

impl SupportNResist {
    pub fn new() -> Self {
        Self {
            past_high: 0.0,
            past_low: 0.0,
            past_close: 0.0,
            ran_before: false,
        }
    }
}

impl StockStrategy for SupportNResist {
    fn run(&mut self, data: &Bar) -> anyhow::Result<StrategyOutput> {
        if !self.ran_before {
            self.ran_before = true;
            self.past_high = data.high_price.to_f64().unwrap();
            self.past_low = data.low_price.to_f64().unwrap();
            self.past_close = data.close_price.to_f64().unwrap();
            return Ok(StrategyOutput::Hold);
        }
        let current_price = data.close_price.to_f64().unwrap();

        let center = (self.past_high + self.past_low + self.past_close) / 3.0;
        let resistance = 2.0 * center - self.past_low;

        if current_price >= resistance {
            return Ok(StrategyOutput::Sell);
        }
        if current_price > center {
            return Ok(StrategyOutput::Buy);
        }
        Ok(StrategyOutput::Hold)
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (
            bincode::serialize(&self).unwrap(),
            "Support and Resist".to_string(),
        )
    }

    fn run_backtest(
        &mut self,
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
    ) -> StrategyOutput {
        if !self.ran_before {
            self.ran_before = true;
            self.past_high = high;
            self.past_low = low;
            self.past_close = close;
            return StrategyOutput::Hold;
        }
        let current_price = close;

        let center = (self.past_high + self.past_low + self.past_close) / 3.0;
        let resistance = 2.0 * center - self.past_low;

        if current_price >= resistance {
            return StrategyOutput::Sell;
        }
        if current_price > center {
            return StrategyOutput::Buy;
        }
        StrategyOutput::Hold
    }
}

//How many bars the Retracement should wait before doing a calculation
const MAXBARS: u16 = 460;

//How much leeway the price has when it comes to the half, failure, and profit zones
const ERROR_MARGAIN: f64 = 0.05;

//TODO, make this one work
#[derive(Serialize, Deserialize, Default)]
pub struct FibonacciRetracement {
    bar_start: f64,
    bar_end: f64,
    current_bar: u16,

    monitoring_mode: bool,

    //All the ranges
    half_way_back_price_upper: f64,
    half_way_back_price_lower: f64,

    failure_price_upper: f64,
    failure_price_lower: f64,

    profit_price_upper: f64,
    profit_price_lower: f64,

    stock_going_up: bool,
}

impl FibonacciRetracement {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StockStrategy for FibonacciRetracement {
    fn run_backtest(
        &mut self,
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
    ) -> StrategyOutput {
        //We should only start monitoring on uptrends
        if self.monitoring_mode {
            //UpTrend continuing, sell
            if close >= self.profit_price_lower && close <= self.profit_price_upper {
                self.monitoring_mode = false;
                info!("Resetting!");
                return StrategyOutput::Sell;
            }

            //Trend in limbo, wait
            if close >= self.half_way_back_price_lower && close <= self.half_way_back_price_upper {
                return StrategyOutput::Buy;
            }

            if close >= self.failure_price_lower && close <= self.failure_price_upper {
                //Uptrend fail, sell stock before too much loss
                self.monitoring_mode = false;
                info!("Resetting!");
                return StrategyOutput::Sell;
            }

            return StrategyOutput::Hold;
        }

        //Get the first bar
        if self.current_bar == 0 {
            self.bar_start = close;
        }

        //Wait until we hit the max, then start monitoring price
        if self.current_bar == MAXBARS {
            self.bar_end = close;

            let difference: f64 = self.bar_end - self.bar_start;

            self.stock_going_up = difference.is_sign_positive();

            //Create the prices to track before setting the monitoring_mode

            self.profit_price_upper = self.bar_end - (difference * (0.236 + ERROR_MARGAIN));
            self.profit_price_lower = self.bar_end - (difference * (0.236 - ERROR_MARGAIN));

            self.half_way_back_price_upper = self.bar_end - (difference * (0.5 + ERROR_MARGAIN));
            self.half_way_back_price_lower =
                self.bar_end - (difference * (0.5 - ERROR_MARGAIN + 0.05));

            self.failure_price_upper = self.bar_end - (difference * (0.618 + ERROR_MARGAIN));
            self.failure_price_lower = self.bar_end - (difference * (0.618 - ERROR_MARGAIN));

            //If there is an uptrend, buy and monitor, else reset
            if self.stock_going_up {
                info!("Detected uptrend, watching");

                info!("Current price: {}", close);
                info!("Difference: {}", difference);
                info!(
                    "Failure Limits: {}, {}",
                    self.failure_price_upper, self.failure_price_lower
                );
                info!(
                    "HalfWayBack Limits: {}, {}",
                    self.half_way_back_price_upper, self.half_way_back_price_lower
                );
                info!(
                    "Profit price Limits: {}, {}",
                    self.profit_price_upper, self.profit_price_lower
                );

                self.monitoring_mode = true;
                self.current_bar = 0;
                return StrategyOutput::Hold;
            } else {
                info!("Detected downtrend, resetting");
                self.current_bar = 0;
                self.monitoring_mode = false;
                return StrategyOutput::Hold;
            }
        }

        self.current_bar += 1;
        StrategyOutput::Hold
    }

    fn run(&mut self, data: &Bar) -> anyhow::Result<StrategyOutput> {
        let close = {
            if data.close_price.to_f64().is_some() {
                data.close_price.to_f64().unwrap()
            } else {
                return Err(Error::msg("Could not convert close price to f64"));
            }
        };

        //We should only start monitoring on uptrends
        if self.monitoring_mode {
            //UpTrend continuing, sell
            if close >= self.profit_price_lower && close <= self.profit_price_upper {
                self.monitoring_mode = false;
                self.current_bar = 0;
                return Ok(StrategyOutput::Sell);
            }

            //Trend in limbo, wait
            if close >= self.half_way_back_price_lower && close <= self.half_way_back_price_upper {
                return Ok(StrategyOutput::Hold);
            }

            if close >= self.failure_price_lower && close <= self.failure_price_upper {
                //Uptrend fail, sell stock before too much loss
                self.monitoring_mode = false;
                self.current_bar = 0;
                return Ok(StrategyOutput::Sell);
            }

            return Ok(StrategyOutput::Hold);
        }

        //Get the first bar
        if self.current_bar == 0 {
            self.bar_start = close;
        }

        //Wait until we hit the max, then start monitoring price
        if self.current_bar == MAXBARS {
            self.bar_end = close;

            let difference: f64 = self.bar_end - self.bar_start;

            self.stock_going_up = difference.is_sign_positive();

            //Create the prices to track before setting the monitoring_mode

            self.profit_price_upper = self.bar_end - (difference * (0.236 + ERROR_MARGAIN));
            self.profit_price_lower = self.bar_end - (difference * (0.236 - ERROR_MARGAIN));

            self.half_way_back_price_upper = self.bar_end - (difference * (0.5 + ERROR_MARGAIN));
            self.half_way_back_price_lower = self.bar_end - (difference * (0.5 - ERROR_MARGAIN));

            self.failure_price_upper = self.bar_end - (difference * (0.618 + ERROR_MARGAIN));
            self.failure_price_lower = self.bar_end - (difference * (0.618 - ERROR_MARGAIN));

            //If there is an uptrend, buy and monitor, else reset
            if self.stock_going_up {
                self.monitoring_mode = true;

                return Ok(StrategyOutput::Buy);
            } else {
                self.current_bar = 0;
                self.monitoring_mode = false;
                return Ok(StrategyOutput::Hold);
            }
        }

        self.current_bar += 1;
        Ok(StrategyOutput::Hold)
    }

    fn save_state(&self) -> (Vec<u8>, String) {
        (bincode::serialize(&self).unwrap(), "Fibonacci".to_string())
    }
}
