# Crain Node

### Rust Setup

To run the node you will need [Rust](./docs/rust-setup.md) programming language installed.

### Commands

Build and run the node in development state:

```sh
cargo run --release -- --dev
```  
or  
```sh
cargo build -r
```
_(let the build finish)_
```
./target/release/crain-node --dev
```
Sometimes you may want to run the node with detailed logging:

```bash
RUST_BACKTRACE=1 ./target/release/crain-node -ldebug --dev
```  

After running any of these commands you should see a detailed log in your terminal. Pay attention to the
```
Idle (...), best: #... , finalized #...
```
part. If the number of _best_ is incrementing it means that the node is producing blocks.

### Node Description
__Framework__  
The node was created using [Substrate](https://substrate.io/) framework.   
- It is based on [Substrate Node Template](https://github.com/substrate-developer-hub/substrate-node-template)

__Consensus__  
The node is designed to be a part of [__Proof of Work__](https://medium.com/swlh/how-does-bitcoin-blockchain-mining-work-36db1c5cb55d) blockchain.  
- To implement the PoW consensus the following pallets were used:
  - [difficulty](https://github.com/kulupu/kulupu/tree/master/frame/difficulty)
  - [consensus-pow](https://paritytech.github.io/substrate/master/sc_consensus_pow/index.html)
- Also a custom PoW algorithm was created. You can find it in `pow` directory of the repository.

__Contracts__  
The node supports smart-contracts written in [__ink!__](https://ink.substrate.io/) language.  
You can find a few contracts in the `contracts` directory of the repository.  
To deploy, call and test smart-contracts the [cargo contract](https://github.com/paritytech/cargo-contract) tool was used.  
There two more ways to interact with contracts running on a local node:
- [Polkadot front-end](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944)
- [Contracts UI node](https://github.com/paritytech/contracts-ui)

__Multiple Nodes__  
If you want to use a multi-node chain, please see the guide:
[Start a Private Network tutorial](https://docs.substrate.io/tutorials/v3/private-network).
