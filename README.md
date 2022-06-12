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

If you run the node without `--dev` key all chain state will be saved to the file on   
your machine and you will have to _purge_ it every time:

```bash
./target/release/crain-node purge-chain
```

Sometimes you may want to run the node with detailed logging:

```bash
RUST_BACKTRACE=1 ./target/release/crain-node -ldebug --dev
```  
### Node Specifications
__Consensus__  
The node is designed to be a part of [__Proof of Work__](https://medium.com/swlh/how-does-bitcoin-blockchain-mining-work-36db1c5cb55d) blockchain.  
The node was created using [Substrate](https://substrate.io/) framework.   
- It is based on [Substrate Node Template](https://github.com/substrate-developer-hub/substrate-node-template)
- To implement the PoW consensus the following pallets were used:
  - [difficulty](https://github.com/kulupu/kulupu/tree/master/frame/difficulty)
  - [consensus-pow](https://paritytech.github.io/substrate/master/sc_consensus_pow/index.html)
- Also a custom PoW algorithm was created. You can find it in `pow` directory of the repository.

__Contracts__  
The node supports smart-contracts written in [__ink!__](https://ink.substrate.io/) language.  
You can find a few contracts in the `contracts` directory of the repository.


### Additional Information
Development chain means that the state of your chain will be in a `tmp` folder while the node is
running. Also, __Alice__ account will be authority and sudo account. The following accounts will be pre-funded:  
- Alice  
- Bob  
- Alice//stash  
- Bob//stash  

Once the node is running locally, you can connect [Polkadot front-end](https://polkadot.js.org/apps/#/explorer?rpc=ws://localhost:9944) to interact with it

If you want to use a multi-node chain, please see the guide:
[Start a Private Network tutorial](https://docs.substrate.io/tutorials/v3/private-network).