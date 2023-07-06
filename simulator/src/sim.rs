use anyhow::Result;
use ethers::providers::Middleware;
use ethers::types::{AccountDiff, Address, BlockNumber, Transaction, H160, H256, I256, U256};
use revm::primitives::{ExecutionResult, Output, TransactTo, B160, U256 as rU256};
use revm::EVM;
use rusty_sando::prelude::fork_db::ForkDB;
use rusty_sando::prelude::PoolVariant;
use rusty_sando::simulate::{
    attach_braindance_module, braindance_address, braindance_controller_address, setup_block_state,
};
use rusty_sando::types::BlockInfo;
use rusty_sando::utils::tx_builder::braindance;
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::data::HistoricalEvent;
use crate::util::{get_pair_tokens, WsClient};
use rusty_sando::{forked_db::fork_factory::ForkFactory, utils::state_diff};

/// Return an evm instance forked from the provided block info and client state
/// with braindance module initialized.
pub async fn fork_evm(client: &WsClient, block_info: &BlockInfo) -> Result<EVM<ForkDB>> {
    let fork_block_num = BlockNumber::Number(block_info.number);
    let fork_block = Some(ethers::types::BlockId::Number(fork_block_num));

    let state_diffs =
        if let Some(sd) = state_diff::get_from_txs(&client, &vec![], fork_block_num).await {
            sd
        } else {
            BTreeMap::<H160, AccountDiff>::new()
        };
    let initial_db = state_diff::to_cache_db(&state_diffs, fork_block, &client)
        .await
        .unwrap(); // TODO: handle unwrap
    let mut fork_factory = ForkFactory::new_sandbox_factory(client.clone(), initial_db, fork_block);
    attach_braindance_module(&mut fork_factory);

    let mut evm = EVM::new();
    evm.database(fork_factory.new_sandbox_fork());
    setup_block_state(&mut evm, block_info);

    Ok(evm)
}

/// Information derived from user's trade tx.
#[derive(Debug, Clone)]
struct UserTradeParams {
    pub pool_variant: PoolVariant,
    pub token_in: Address,
    pub token_out: Address,
    pub amount0_sent: I256,
    pub amount1_sent: I256,
    pub pool: Address,
}

/// Returns None if trade params can't be derived
async fn derive_trade_params(
    client: &WsClient,
    tx: Transaction,
    event: &HistoricalEvent,
) -> Result<Option<UserTradeParams>> {
    let uniswap_topics = vec![
        // univ3
        // Swap(address,address,int256,int256,uint160,uint128,int24)
        H256::from_str("0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67")
            .unwrap(),
        // univ2
        // Swap(address,uint256,uint256,uint256,uint256,address)
        H256::from_str("0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822")
            .unwrap(),
    ];

    // 0. get pool address from event, relying on mev-share hints
    println!("hint logs {:?}", event.hint.logs);
    let swap_log = event
        .hint
        .logs
        .iter()
        .find(|log| uniswap_topics.contains(&log.topics[0]))
        .unwrap(); // TODO: handle unwrap
    let pool_address = swap_log.address;
    let swap_topic = swap_log.topics[0];
    println!("pool address: {:?}", pool_address);

    // 1. derive pool variant from event log topics
    println!("swap topic: {:?}", swap_topic);
    let univ3 =
        H256::from_str("0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67")
            .unwrap();
    let pool_variant = if swap_topic.eq(&univ3) {
        PoolVariant::UniswapV3
    } else {
        PoolVariant::UniswapV2 // assume events are pre-screened, so all non-V3 events are V2
    };

    // 2. get token addrs from pool address
    let (token0, token1) = get_pair_tokens(client, pool_address).await?;
    println!("token0\t{:?}\ntoken1\t{:?}", token0, token1);

    // 3. derive trade direction from (full) tx logs
    let tx_receipt = client.get_transaction_receipt(tx.hash).await?.unwrap(); // TODO: handle unwrap
    println!("tx ({:?})\nLOGZZZ: {:?}", tx.hash, tx_receipt.logs);
    let swap_log = tx_receipt
        .logs
        .into_iter()
        .find(|log| log.topics.contains(&swap_topic));
    println!("swap_log: {:?}", swap_log);
    println!("pool variant: {:?}", pool_variant);

    let swap_log = swap_log.ok_or(anyhow::format_err!(
        "no swap logs found for tx {:?}",
        tx.hash
    ))?;

    let (amount0_sent, amount1_sent) = match pool_variant {
        PoolVariant::UniswapV3 => {
            let amount0 = I256::from_raw(U256::from_big_endian(&swap_log.data[0..32]));
            let amount1 = I256::from_raw(U256::from_big_endian(&swap_log.data[32..64]));
            // let sqrt_price = U256::from_big_endian(&swap_log.data[64..96]); // u160
            // let liquidity = U256::from_big_endian(&swap_log.data[96..128]); // u128
            // let tick = I256::from_raw(U256::from_big_endian(&swap_log.data[128..160])); // i24
            println!("user trade data ===========================");
            println!("amount0\t\t{:?}", amount0);
            println!("amount1\t\t{:?}", amount1);
            // println!("sqrtPriceX96\t{:?}", sqrt_price);
            // println!("liquidity\t{:?}", liquidity);
            // println!("tick\t\t{:?}", tick);
            (
                if amount0.le(&0.into()) {
                    0.into()
                } else {
                    amount0
                },
                if amount1.le(&0.into()) {
                    0.into()
                } else {
                    amount1
                },
            )
        }
        PoolVariant::UniswapV2 => {
            // let amount0_in = U256::from_big_endian(&swap_log.data[0..32]);
            // let amount1_in = U256::from_big_endian(&swap_log.data[32..64]);
            let amount0_out = I256::from_raw(U256::from_big_endian(&swap_log.data[64..96]));
            let amount1_out = I256::from_raw(U256::from_big_endian(&swap_log.data[96..128]));
            (amount0_out, amount1_out)
        }
    };

    let swap_0_for_1 = amount0_sent.gt(&0.into());

    println!(
        "***\nuser swaps {} for {}\n***",
        if swap_0_for_1 { token0 } else { token1 },
        if swap_0_for_1 { token1 } else { token0 }
    );

    Ok(Some(UserTradeParams {
        pool_variant,
        token_in: if swap_0_for_1 { token0 } else { token1 },
        token_out: if swap_0_for_1 { token1 } else { token0 },
        amount0_sent,
        amount1_sent,
        pool: pool_address,
    }))
}

/// Find the optimal backrun for a given tx.
pub async fn find_optimal_backrun(
    client: &WsClient,
    user_tx: Transaction,
    event: &HistoricalEvent,
    block_info: &BlockInfo,
) -> Result<()> {
    let mut evm = fork_evm(client, block_info).await?;
    sim_bundle(&mut evm, vec![user_tx.to_owned()]).await?;

    let params = derive_trade_params(client, user_tx.to_owned(), event).await?;
    if let Some(params) = params {
        // TODO: change `amount_in` over many iterations to find optimal trade
        let amount_in = params.amount0_sent.max(params.amount1_sent);

        /* Buy tokens on one exchange. */
        let res = commit_braindance_swap(
            &mut evm,
            params.pool_variant,
            amount_in.unsigned_abs(),
            params.pool,
            params.token_out,
            params.token_in,
            block_info.base_fee,
            None,
        );
        println!("braindance 1 completed. {:?}", res);

        /* Sell them on other exchange. */
        let res = commit_braindance_swap(
            &mut evm,
            params.pool_variant,
            amount_in.unsigned_abs(),
            params.pool,
            params.token_in,
            params.token_out,
            block_info.base_fee,
            None,
        );
        println!("braindance 2 completed. {:?}", res);
    } else {
        println!("no params found for tx {:?}", user_tx.hash);
    }

    // TODO: return something useful
    Ok(())
}

/// Simulate a bundle of transactions, returns array containing each tx's simulation result.
pub async fn sim_bundle(
    evm: &mut EVM<ForkDB>,
    signed_txs: Vec<Transaction>,
) -> Result<Vec<ExecutionResult>> {
    let mut results = vec![];
    for tx in signed_txs {
        evm.env.tx.caller = B160::from(tx.from);
        evm.env.tx.transact_to = TransactTo::Call(B160::from(tx.to.unwrap_or_default().0));
        evm.env.tx.data = tx.input.0;
        evm.env.tx.value = tx.value.into();
        evm.env.tx.chain_id = tx.chain_id.map(|id| id.as_u64());
        evm.env.tx.nonce = Some(tx.nonce.as_u64());
        evm.env.tx.gas_limit = tx.gas.as_u64();
        match tx.transaction_type {
            Some(ethers::types::U64([0])) => {
                evm.env.tx.gas_price = tx.gas_price.unwrap_or_default().into();
            }
            Some(_) => {
                // type-2 tx
                evm.env.tx.gas_priority_fee = tx.max_priority_fee_per_gas.map(|fee| fee.into());
                evm.env.tx.gas_price = tx.max_fee_per_gas.unwrap_or_default().into();
            }
            None => {
                // legacy tx
                evm.env.tx.gas_price = tx.gas_price.unwrap_or_default().into();
            }
        }
        let res = evm.transact_commit();
        if let Ok(res) = res {
            results.push(res.to_owned());
            println!("simulated transaction: {:?}", res);
        } else {
            println!("failed to simulate transaction: {:?}", res);
        }
    }

    Ok(results)
}

/// Run a swap against evm. Returns balance of token_out after tx is executed.
pub fn commit_braindance_swap(
    evm: &mut EVM<ForkDB>,
    pool_variant: PoolVariant,
    amount_in: U256,
    target_pool: Address,
    token_in: Address,
    token_out: Address,
    base_fee: U256,
    nonce: Option<u64>,
) -> Result<U256> {
    let swap_data = match pool_variant {
        PoolVariant::UniswapV2 => {
            braindance::build_swap_v2_data(amount_in, target_pool, token_in, token_out)
        }
        PoolVariant::UniswapV3 => braindance::build_swap_v3_data(
            amount_in.as_u128().into(),
            target_pool,
            token_in,
            token_out,
        ),
    };

    evm.env.tx.caller = braindance_controller_address();
    evm.env.tx.transact_to = TransactTo::Call(braindance_address().0.into());
    evm.env.tx.data = swap_data.0;
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = base_fee.into();
    evm.env.tx.value = rU256::ZERO;
    evm.env.tx.nonce = nonce;

    let res = match evm.transact_commit() {
        Ok(res) => res,
        Err(e) => return Err(anyhow::anyhow!("failed to commit swap: {:?}", e)),
    };
    let output = match res.to_owned() {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o,
            Output::Create(o, _) => o,
        },
        ExecutionResult::Revert { output, .. } => {
            return Err(anyhow::anyhow!("swap reverted: {:?}", output))
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(anyhow::anyhow!("swap halted: {:?}", reason))
        }
    };
    let (_amount_out, balance) = match pool_variant {
        PoolVariant::UniswapV2 => match braindance::decode_swap_v2_result(output.into()) {
            Ok(output) => output,
            Err(e) => return Err(anyhow::anyhow!("failed to decode swap result: {:?}", e)),
        },
        PoolVariant::UniswapV3 => match braindance::decode_swap_v3_result(output.into()) {
            Ok(output) => output,
            Err(e) => return Err(anyhow::anyhow!("failed to decode swap result: {:?}", e)),
        },
    };
    Ok(balance)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::{get_block_info, get_ws_client};
    use anyhow::Result;
    use ethers::providers::Middleware;

    async fn setup_test_evm(client: &WsClient, block_num: u64) -> Result<EVM<ForkDB>> {
        let block_info = get_block_info(&client, block_num).await?;
        fork_evm(&client, &block_info).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn it_simulates_tx() -> Result<()> {
        let client = get_ws_client("ws://localhost:8545".to_owned()).await?;
        let block_num = client.get_block_number().await?;
        let mut evm = setup_test_evm(&client, block_num.as_u64() - 1).await?;
        let block = client.get_block(block_num).await?.unwrap();
        let tx_hash = block.transactions[0];
        let tx = client.get_transaction(tx_hash).await?.unwrap();
        let res = sim_bundle(&mut evm, vec![tx]).await;
        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(res[0].is_success());
        Ok(())
    }
}
