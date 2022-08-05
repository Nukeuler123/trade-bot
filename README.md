

# What Is this?
This program allows the user to buy and sell stocks using the alpaca trading API automatically using different sets of strategies based on their config settings (No shorting yet)

## How to compile
1. Intall the rust toolchain [here](https://www.rust-lang.org/tools/install)
2. git clone this repository
3. run cargo build --release
4. Done! The program should be in the /target directory somehwere

## How do I run the bot?
1. Copy the Config.toml file from the root of the repository
2. Fill in information in the config
3. Add more stocks as needed
4. Set testing mode to false if running in production mode
5. Done!

Note: The bot will create a new folder called stock_state this is a local DB used to store the stock montor's state in case of loss of power or a reboot

## Will I turn  profit?
Maybe, nothing is guaranteed in life or the stock market so I can't promise anything.

Though I would recommend trying the bot out with paper trading first and fine tuning what stocks/etfs you want to invest in.

Im also not responsible for any bugs in the code that may cause losses in profit. Im saying this to cover myself. 

## Where should I host my bot?
I don't know, you could rent a cheap VPS anywhere. The program isn't that computationally demanding.

## Can we get X strategy?
Sure! Code it and test it yourself before submitting a PR

## What strategies can I currently use?
1. Single Moving Average
2. Two Moving Averages
3. Support and Resist
4. Fibonacci (Unfinished DO NOT USE!)

## Found a bug! 
Make an issue and explain how the bug happened, if you can provide logs and configs with confidential parts redacted