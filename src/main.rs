//! # Exbrid
//!
//! This program runs a full Ethereum node using Reth (via ExEx) to monitor finalized blocks.
//! It captures the block number and hash of each finalized Ethereum block and sends it to a
//! Polkadot-based Substrate chain (e.g., Paseo testnet) using Subxt (which uses a Smoldot light
//! client internally).
//!
//! The communication is done via broadcast channels between the Ethereum node component and the
//! Subxt task. The Subxt task signs and submits a `system.remark_with_event` transaction containing
//! the block hash to the Polkadot chain.

use futures_util::{FutureExt, TryStreamExt};
use reth::{
    api::FullNodeComponents, builder::NodeTypes, primitives::EthPrimitives,
    providers::BlockIdReader,
};
use reth_exex::ExExContext;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::info;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{ready, Context, Poll},
};
use tokio::{sync::broadcast, task};

/// Subxt imports for Polkadot communication
use std::time::Duration;
use subxt::{
    client::OnlineClient,
    lightclient::{ChainConfig, LightClient},
    PolkadotConfig,
};
use subxt_signer::bip39::Mnemonic;

/// Generated Rust interface to interact with the Polkadot based chain using metadata.
/// Change the metadata file according to our chain.
#[subxt::subxt(runtime_metadata_path = "paseo_metadata.scale")]
pub mod polkadot {}

/// Type alias for Ethereum block info: (block number, block hash)
type BlockHashInfo = (alloy_primitives::BlockNumber, alloy_primitives::B256);

/// Broadcast channel for block info
type BlockHashSender = broadcast::Sender<BlockHashInfo>;
type BlockHashReceiver = broadcast::Receiver<BlockHashInfo>;

/// Shared state structure between ExEx and Subxt task
struct SharedState {
    current_block: Option<BlockHashInfo>,
}

/// Implementation of ExEx extension to capture finalized Ethereum blocks
struct MyExEx<Node: FullNodeComponents> {
    ctx: ExExContext<Node>,
    shared_state: Arc<Mutex<SharedState>>,
    block_hash_sender: BlockHashSender,
}

/// Implements the ExEx trait for custom event polling logic
impl<Node: FullNodeComponents> MyExEx<Node> {
    fn new(
        ctx: ExExContext<Node>,
        shared_state: Arc<Mutex<SharedState>>,
        sender: BlockHashSender,
    ) -> Self {
        Self { ctx, shared_state, block_hash_sender: sender }
    }
}

impl<Node: FullNodeComponents<Types: NodeTypes<Primitives = EthPrimitives>>> Future
    for MyExEx<Node>
{
    type Output = eyre::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        while let Some(notification) = ready!(this.ctx.notifications.try_next().poll_unpin(cx))? {
            if let Some(..) = notification.committed_chain() {
                // Retrieve finalized block number and hash
                if let Ok(Some(finalized)) = this.ctx.provider().finalized_block_num_hash() {
                    // Update shared state
                    let block_info = (finalized.number, finalized.hash);
                    {
                        let mut shared = this.shared_state.lock().unwrap();
                        shared.current_block = Some(block_info);
                    }

                    // Notify the Subxt task
                    let _ = this.block_hash_sender.send(block_info);

                    info!(
                        "Finalized block number: {}, hash: {:?}",
                        finalized.number, finalized.hash
                    );
                } else {
                    info!("No finalized block information available.");
                }
            }
        }
        Poll::Ready(Ok(()))
    }
}

/// Subxt + Smoldot light client logic to listen and relay block hashes to Polkadot based chains
async fn run_subxt_client(receiver: BlockHashReceiver) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting light client...");

    // Load chain spec
    let chain_spec = include_str!("../paseo.raw.json");
    println!("Loaded chain spec");

    // Configure the bootnodes
    println!("Setting up bootnode connection");
    let chain_config = ChainConfig::chain_spec(chain_spec).set_bootnodes([
		"/dns/paseo.bootnode.amforc.com/tcp/30333/wss/p2p/12D3KooWFD81HC9memUwuGMLvhDDEfmXjn6jC4n7zyNs3vToXapS",
		"/dns/paseo.bootnode.amforc.com/tcp/30344/p2p/12D3KooWFD81HC9memUwuGMLvhDDEfmXjn6jC4n7zyNs3vToXapS",
		"/dns/boot.stake.plus/tcp/43334/wss/p2p/12D3KooWNhgAC3hjZHxaT52EpPFZohkCL1AHFAijqcN8xB9Rwud2",
		"/dns/boot.stake.plus/tcp/43333/p2p/12D3KooWNhgAC3hjZHxaT52EpPFZohkCL1AHFAijqcN8xB9Rwud2",
		"/dns/boot.metaspan.io/tcp/36017/wss/p2p/12D3KooWSW6nDfM3SS8rUtjMyjdszivK31bu4a1sRngGa2hFETz7",
		"/dns/boot.metaspan.io/tcp/36018/p2p/12D3KooWSW6nDfM3SS8rUtjMyjdszivK31bu4a1sRngGa2hFETz7",
		"/dns/paseo.bootnodes.polkadotters.com/tcp/30538/p2p/12D3KooWPbbFy4TefEGTRF5eTYhq8LEzc4VAHdNUVCbY4nAnhqPP",
		"/dns/paseo.bootnodes.polkadotters.com/tcp/30540/wss/p2p/12D3KooWPbbFy4TefEGTRF5eTYhq8LEzc4VAHdNUVCbY4nAnhqPP",
		"/dns/boot-node.helikon.io/tcp/10020/p2p/12D3KooWBetfzZpf6tGihKrqCo5z854Ub4ZNAUUTRT6eYHNh7FYi",
		"/dns/boot-node.helikon.io/tcp/10022/wss/p2p/12D3KooWBetfzZpf6tGihKrqCo5z854Ub4ZNAUUTRT6eYHNh7FYi",
		"/dns/boot.gatotech.network/tcp/33400/p2p/12D3KooWEvz5Ygv3MhCUNTVQbUTVhzhvf4KKcNoe5M5YbVLPBeeW",
		"/dns/boot.gatotech.network/tcp/35400/wss/p2p/12D3KooWEvz5Ygv3MhCUNTVQbUTVhzhvf4KKcNoe5M5YbVLPBeeW",
		"/dns/paseo-bootnode.turboflakes.io/tcp/30630/p2p/12D3KooWMjCN2CrnN71hAdehn6M2iYKeGdGbZ1A3SKhf4hxrgG9e",
		"/dns/paseo-bootnode.turboflakes.io/tcp/30730/wss/p2p/12D3KooWMjCN2CrnN71hAdehn6M2iYKeGdGbZ1A3SKhf4hxrgG9e",
		"/dns/pso16.rotko.net/tcp/33246/p2p/12D3KooWRH8eBMhw8c7bucy6pJfy94q4dKpLkF3pmeGohHmemdRu",
		"/dns/pso16.rotko.net/tcp/35246/wss/p2p/12D3KooWRH8eBMhw8c7bucy6pJfy94q4dKpLkF3pmeGohHmemdRu",
	])?;

    // Start Smoldot-based light client
    let (_light_client, chain_rpc) = LightClient::relay_chain(chain_config)?;

    // Add the crucial wait for sync
    println!("Waiting for light client to sync (60 seconds)...");
    tokio::time::sleep(Duration::from_secs(60)).await;

    println!("Creating API client...");
    // Create Subxt client from the light client RPC
    let api = OnlineClient::<PolkadotConfig>::from_rpc_client(chain_rpc).await?;

    println!("API client created successfully");

    // Constant mnemonic phrase for signing the extrinsic (replace with your seed phrase!)
    let phrase = Mnemonic::parse(
        "girl run radar student point expect segment process ocean ability artwork ahead",
    )
    .expect("Valid phrase");
    let from = subxt_signer::sr25519::Keypair::from_phrase(&phrase, None).unwrap();

    // Wait for balance check to succeed (ensures Smoldot is synced)
    let account_id = subxt::utils::AccountId32::from(from.public_key());

    const MAX_RETRIES: u32 = 30;
    const RETRY_DELAY_SECS: u64 = 2;

    let mut synced = false;
    for _ in 0..MAX_RETRIES {
        match api
            .storage()
            .at_latest()
            .await?
            .fetch(&polkadot::storage().system().account(&account_id))
            .await
        {
            Ok(Some(..)) => {
                println!("✅ Smoldot is ready ");
                synced = true;
                break;
            }
            _ => {
                tokio::time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
            }
        }
    }

    if !synced {
        println!("❌ Smoldot did not sync within the expected time. Exiting...");
        return Ok(());
    }

    // Subscribe to block info broadcast channel
    let mut rx = receiver;

    // Process incoming Ethereum block hashes and submit as remarks to Polkadot based chain
    while let Ok((block_number, block_hash)) = rx.recv().await {
        // Create a remark that includes the finalized block hash from Ethereum
        let remark_text =
            format!("ETH finalized block: number={}, hash={}", block_number, block_hash);
        let remark = remark_text.as_bytes().to_vec();

        // Build the remark extrinsic
        let remark_tx = polkadot::tx().system().remark_with_event(remark.clone());

        println!("Submitting system.remark transaction with ETH block info...");
        match api.tx().sign_and_submit_then_watch_default(&remark_tx, &from).await {
            Ok(progress) => {
                match progress.wait_for_finalized_success().await {
                    Ok(events) => {
                        // Find a system.remark event
                        match events.find_first::<polkadot::system::events::ExtrinsicSuccess>() {
                            Ok(Some(event)) => {
                                println!("System remark transaction successful: {event:?}")
                            }
                            Ok(None) => println!("No ExtrinsicSuccess event found"),
                            Err(e) => println!("Failed to parse events: {:?}", e),
                        }
                    }
                    Err(e) => println!("Error waiting for transaction to be finalized: {:?}", e),
                }
            }
            Err(e) => println!("Error submitting transaction: {:?}", e),
        }
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    // Setup broadcast channel and shared state
    let (tx, _rx) = broadcast::channel::<BlockHashInfo>(100);
    let shared_state = Arc::new(Mutex::new(SharedState { current_block: None }));

    // Start Reth node using ExEx and spawn Subxt task
    reth::cli::Cli::parse_args().run(async move |builder, _| {
        // Spawn Subxt background task
        let subxt_rx = tx.subscribe();
        let subxt_task = task::spawn(async move {
            if let Err(e) = run_subxt_client(subxt_rx).await {
                eprintln!("Subxt client error: {:?}", e);
            }
        });

        // Install and launch ExEx extension inside the Ethereum node
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("block-logger", async move |ctx| {
                Ok(MyExEx::new(ctx, shared_state.clone(), tx.clone()))
            })
            .launch()
            .await?;

        // Just let the subxt task run in the background and wait for the ethereum node to exit
        let result = handle.wait_for_node_exit().await;

        // Stop Subxt task
        subxt_task.abort();

        result
    })
}
