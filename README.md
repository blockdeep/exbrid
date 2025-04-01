# 🧪 Exbrid

Exbrid is an **experiment** exploring an alternate approach to message passing between Ethereum and Polkadot by embedding a **Polkadot light client (Smoldot)** directly inside an **Ethereum node (Reth)** using its plugin system **ExEx**.

Instead of relying on separate off-chain relayers, this prototype shows how finalized Ethereum block information can be captured from within the Ethereum node itself and submitted directly to a Polkadot-based Substrate chain (like [Paseo](https://github.com/paritytech/polkadot-sdk)) using [Subxt](https://github.com/paritytech/subxt), with Smoldot as the embedded client.

## Initial Research

> ❗ **Disclaimer:** This is an early-stage prototype for research and exploration purposes only. It is not intended for production or secure cross-chain message passing.

The following questions are yet to be answered before it can be used safely:

* How will the Substrate/Polkadot runtime know that the message containing the finalized hash is actually correct?
* Can we get around the fact that two sides need to run an onchain light client of one another?
* Maybe other security or trust assumptions that we must validate. TBD.

---

## 🧭 Project Overview

- Runs a full Ethereum node using [Reth](https://github.com/paradigmxyz/reth) with a custom [ExEx](https://github.com/paradigmxyz/reth-exex) extension.
- Captures finalized Ethereum block numbers and hashes.
- Uses [Smoldot](https://github.com/paritytech/smoldot) to run a lightweight Polkadot client embedded in the same process.
- Uses [Subxt](https://github.com/paritytech/subxt) to submit `system.remark_with_event` transactions to a Substrate-based chain with the finalized Ethereum block info.

This allows Ethereum to directly push information to Polkadot with **no external services or full Polkadot node dependencies**.

---

## 📁 Structure

```
.
├── src/
│   └── main.rs                 # Reth ExEx plugin + Subxt integration
├── paseo_metadata.scale        # Subxt metadata for Paseo chain
├── paseo.raw.json              # Chain spec for Smoldot light client
├── Cargo.toml
├── README.md
```

---

## ⚙️ Requirements

- Rust >= 1.73
- [Subxt CLI](https://github.com/paritytech/subxt) to generate metadata
- [`protoc`](https://grpc.io/docs/protoc-installation/) if compiling Smoldot from source
- [`cast`](https://book.getfoundry.sh/cast/) (optional) to send test Ethereum transactions
- OpenSSL (for generating JWT secrets when working with Holesky)

---

## 🔬 Running the Experiment Locally (Dev Mode)

### 🧹 Optional Cleanup (Reset Dev Chain)

```bash
pkill -f reth
rm -rf "/Users/<username>/Library/Application Support/reth/dev"
rm -rf "/Users/<username>/Library/Caches/reth"
```

### ▶️ Start the Node in Dev Mode

```bash
cargo run -- node --dev --http
```

This will:

- Start a Reth dev node
- Capture finalized Ethereum blocks
- Run Smoldot light client to connect to Paseo
- Submit finalized block data to Polkadot via `remark_with_event`

---

### ⛽ Trigger Block Production (in Dev Mode)

Dev mode only produces blocks when new transactions are submitted.

Use `cast` to send a dummy transaction and trigger block production:

```bash
cast send \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
  --value 0 \
  0x000000000000000000000000000000000000dEaD \
  --rpc-url http://localhost:8545
```

> **Important:** Only submit this transaction **after** you see this log in your terminal:
>
> ```
> ✅ Smoldot is ready
> ```
>
> This indicates that the light client is fully synced and ready to submit transactions.

---

## 🔄 End-to-End Flow Summary

```
Ethereum (Reth Node)
    │
    └───▶ ExEx captures finalized block
               │
               └───▶ Broadcasts (block_number, block_hash)
                            │
                            ▼
                 Subxt + Smoldot light client
                            │
                            └───▶ Submit remark to Polkadot chain
```

---

## 🧪 Holesky Testnet Setup (Optional)

This project can also be tested on Ethereum Holesky testnet instead of `--dev` mode.

### 1. Generate JWT Secret

```bash
openssl rand -hex 32 > /tmp/jwt/jwt.hex
```

### 2. Start Reth on Holesky

```bash
cargo run -- node \
  --chain holesky \
  --authrpc.jwtsecret /tmp/jwt/jwt.hex \
  --http
```

### 3. Start Lighthouse for Beacon Node Sync

```bash
lighthouse bn \
  --network holesky \
  --purge-db \
  --datadir /tmp/lighthouse-holesky \
  --execution-endpoint http://localhost:8551 \
  --execution-jwt /tmp/jwt/jwt.hex \
  --checkpoint-sync-url https://holesky.beaconstate.info/
```

This setup allows finalized blocks from Holesky to be captured and relayed to a Substrate chain.

---

## 🛠 Setup Notes

### 📥 Download Paseo Chain Spec

```bash
curl -o paseo.raw.json https://paseo-r2.zondax.ch/chain-specs/paseo.raw.json
```

### ⚙️ Generate Subxt Metadata for Paseo

```bash
subxt codegen --url https://rpc.paseo.nodestake.top --output paseo_metadata.scale
```

---

## 🔐 Key Management (Experimental Only)

The current prototype uses a **hardcoded test mnemonic** to sign transactions:

```rust
let phrase = Mnemonic::parse("girl run radar student point expect segment process ocean ability artwork ahead").unwrap();
```

> ⚠️ This is insecure and must be replaced with proper key management (e.g., remote signer, vault integration) if the approach is ever extended toward production usage.

---

## 👨‍🔬 Purpose

This project is not intended to be a fully-featured bridge, but rather a **proof-of-concept** and a **technical experiment** that demonstrates:

- The feasibility of embedding a Polkadot light client into Ethereum
- Relaying finalized data across chains without external services
- Leveraging Reth’s ExEx system as a programmable Ethereum environment
