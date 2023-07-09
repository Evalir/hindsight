use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ethers::types::Transaction;
use simulator::{config::Config, hindsight::HindsightFactory};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// does testing things
    Test {
        /// lists test values
        #[arg(short, long)]
        list: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    let cli = Cli::parse();

    match cli.debug {
        0 => {
            println!("no debug");
        }
        1 => {
            println!("debug 1");
        }
        2 => {
            println!("debug 2");
        }
        _ => {
            println!("max debug");
        }
    }
    match cli.command {
        Some(Commands::Test { list }) => {
            if list {
                println!("test list");
            } else {
                println!("test");
            }
        }
        None => {
            println!("no command");
        }
    }
    if let Some(name) = cli.name {
        println!("name: {}", name);
    }
    if let Some(config) = cli.config.as_deref() {
        println!("config: {:?}", config.display());
    }

    let hindsight = HindsightFactory::new().init(config.to_owned()).await?;

    println!("cache events: {:?}", hindsight.event_map.len());
    println!("cache txs: {:?}", hindsight.cache_txs.len());

    println!(
        "oohh geeez\nauth signer\t{:?}\nrpc url\t\t{:?}",
        config.auth_signer_key, config.rpc_url_ws
    );

    // let mut thread_handlers = vec![];
    let txs: Vec<Transaction> = vec![serde_json::from_str(
        r#"{
            "hash": "0x9c04a13b7b4b2a05123ef6ee796a030d71f205b04a4ca279ff8085acb37e0b4e",
            "nonce": "0x3",
            "blockHash": "0x6ac3b696c81125e781e8931cb575a38a6a18e0fd244ad9aba6307b0cf640aa72",
            "blockNumber": "0x10cbd94",
            "transactionIndex": "0x52",
            "from": "0xd4f74edd038f9a25208a0680d75602a5ea41038d",
            "to": "0x1111111254eeb25477b68fb85ed929f73a960582",
            "value": "0x0",
            "gasPrice": "0x41dc418ac",
            "gas": "0x65f4d",
            "input": "0x12aa3caf00000000000000000000000092f3f71cef740ed5784874b8c70ff87ecdf33588000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000993864e43caa7f7f12953ad6feb1d1ca635b875f00000000000000000000000092f3f71cef740ed5784874b8c70ff87ecdf33588000000000000000000000000d4f74edd038f9a25208a0680d75602a5ea41038d000000000000000000000000000000000000000000000000000000000823a39d00000000000000000000000000000000000000000000000df3e1320d907fb53a000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000018200006800004e80206c4eca27a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48b4f34d09124b8c9712957b76707b42510041ecbb0000000000000000000000000000000000000000000000000000000000042ad20020d6bdbf78a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800a007e5c0d20000000000000000000000000000000000000000000000000000f600008f0c20a0b86991c6218b36c1d19d4a2e9eb0ce3606eb483aa370aacf4cb08c7e1e7aa8e8ff9418d73c7e0f6ae40711b8002dc6c03aa370aacf4cb08c7e1e7aa8e8ff9418d73c7e0f424485f89ea52839fdb30640eb7dd7e0078e12fb00000000000000000000000000000000000000000000000000f36d677c660ec7a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800206ae4071138002dc6c0424485f89ea52839fdb30640eb7dd7e0078e12fb1111111254eeb25477b68fb85ed929f73a96058200000000000000000000000000000000000000000000000df3e1320d907fb53ac02aaa39b223fe8d0a0e5c4f27ead9083c756cc213dbfa98",
            "v": "0x0",
            "r": "0x26a0afb5560b34a0f78921ad33625d71c457d75df088ba542abd501004f13214",
            "s": "0x10b1fefc81664e28526b47ca678c50a32c7d72b9ee4155297e1981a7e34a3daf",
            "type": "0x2",
            "accessList": [],
            "maxPriorityFeePerGas": "0x14dc9380",
            "maxFeePerGas": "0x65a03c400",
            "chainId": "0x1"
          }"#,
    )?];
    println!("txs: {:?}", txs.len());

    hindsight.process_orderflow().await?;

    Ok(())
}
