// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Kulupu.
//
// Copyright (c) 2019-2020 Wei Tang.
//
// Kulupu is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Kulupu is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Kulupu. If not, see <http://www.gnu.org/licenses/>.

pub mod compute;
pub mod weak_sub;

use codec::{Decode, Encode};
use crain_pow_consensus::PowAlgorithm;
use crain_primitives::{Difficulty};
use parking_lot::Mutex;
use rand::{rngs::SmallRng, thread_rng, SeedableRng};
use sc_client_api::{backend::AuxStore, blockchain::HeaderBackend};
use sc_keystore::LocalKeystore;
use sp_api::ProvideRuntimeApi;
use sp_consensus_pow::{DifficultyApi, Seal as RawSeal};
use sp_core::{blake2_256, H256, U256};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto};


use std::{
	sync::Arc,
	time::Instant,
};

use crate::compute::{ComputeMode, ComputeV1, ComputeV2, SealV1, SealV2};

// Exported module of the whole app
pub mod app {
	use sp_std::convert::TryFrom;
	use sp_application_crypto::{app_crypto, sr25519};
	use sp_core::crypto::KeyTypeId;

	pub const ID: KeyTypeId = KeyTypeId(*b"crn1");
	
	// TODO ensure that this line is compiled in std
	app_crypto!(sr25519, ID);
}

/// Checks whether the given hash is above difficulty.
pub fn is_valid_hash(hash: &H256, difficulty: Difficulty) -> bool {
	let num_hash = U256::from(&hash[..]);
	let (_, overflowed) = num_hash.overflowing_mul(difficulty);

	!overflowed
}

pub fn key_hash<B, C>(
	client: &C,
	parent: &BlockId<B>,
) -> Result<H256, crain_pow_consensus::Error<B>>
where
	B: BlockT<Hash = H256>,
	C: HeaderBackend<B>,
{
	const PERIOD: u64 = 4096; // ~2.8 days
	const OFFSET: u64 = 128; // 2 hours

	let parent_header = client
		.header(*parent)
		.map_err(|e| {
			crain_pow_consensus::Error::Environment(format!("Client execution error: {:?}", e))
		})?
		.ok_or(crain_pow_consensus::Error::Environment(
			"Parent header not found".to_string(),
		))?;
	let parent_number = UniqueSaturatedInto::<u64>::unique_saturated_into(*parent_header.number());

	let mut key_number = parent_number.saturating_sub(parent_number % PERIOD);
	if parent_number.saturating_sub(key_number) < OFFSET {
		key_number = key_number.saturating_sub(PERIOD);
	}

	let mut current = parent_header;
	while UniqueSaturatedInto::<u64>::unique_saturated_into(*current.number()) != key_number {
		current = client
			.header(BlockId::Hash(*current.parent_hash()))
			.map_err(|e| {
				crain_pow_consensus::Error::Environment(format!("Client execution error: {:?}", e))
			})?
			.ok_or(crain_pow_consensus::Error::Environment(format!(
				"Block with hash {:?} not found",
				current.hash()
			)))?;
	}

	Ok(current.hash())
}

pub enum RandomXAlgorithmVersion {
	V1,
	V2,
}

pub struct RandomXAlgorithm<C> {
	client: Arc<C>,
}

impl<C> RandomXAlgorithm<C> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client }
	}
}

impl<C> Clone for RandomXAlgorithm<C> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
		}
	}
}

impl<B> From<compute::Error> for crain_pow_consensus::Error<B>
where
	B: sp_runtime::traits::Block,
{
	fn from(e: compute::Error) -> Self {
		crain_pow_consensus::Error::<B>::Other(e.description().to_string())
	}
}

impl<B: BlockT<Hash = H256>, C> PowAlgorithm<B> for RandomXAlgorithm<C>
where
	C: HeaderBackend<B> + AuxStore + ProvideRuntimeApi<B>,
	C::Api: DifficultyApi<B, Difficulty>,
{
	type Difficulty = Difficulty;

	fn difficulty(&self, parent: H256) -> Result<Difficulty, crain_pow_consensus::Error<B>> {
		let difficulty = self
			.client
			.runtime_api()
			.difficulty(&BlockId::Hash(parent))
			.map_err(|e| {
				crain_pow_consensus::Error::Environment(format!(
					"Fetching difficulty from runtime failed: {:?}",
					e
				))
			});

		difficulty
	}

	fn break_tie(&self, own_seal: &RawSeal, new_seal: &RawSeal) -> bool {
		blake2_256(&own_seal[..]) > blake2_256(&new_seal[..])
	}

	fn verify(
		&self,
		parent: &BlockId<B>,
		pre_hash: &H256,
		pre_digest: Option<&[u8]>,
		seal: &RawSeal,
		difficulty: Difficulty,
	) -> Result<bool, crain_pow_consensus::Error<B>> {

		let version = RandomXAlgorithmVersion::V2;

		let key_hash = key_hash(self.client.as_ref(), parent)?;

		match version {
			// TODO This branch will never fire
			RandomXAlgorithmVersion::V1 => {
				let seal = match SealV1::decode(&mut &seal[..]) {
					Ok(seal) => seal,
					Err(_) => return Ok(false),
				};

				let compute = ComputeV1 {
					key_hash,
					difficulty,
					pre_hash: *pre_hash,
					nonce: seal.nonce,
				};

				// No pre-digest check is needed for V1 algorithm.

				let (computed_seal, computed_work) = compute.seal_and_work(ComputeMode::Sync)?;

				if computed_seal != seal {
					return Ok(false);
				}

				if !is_valid_hash(&computed_work, difficulty) {
					return Ok(false);
				}

				Ok(true)
			}
			RandomXAlgorithmVersion::V2 => {
				let seal = match SealV2::decode(&mut &seal[..]) {
					Ok(seal) => seal,
					Err(_) => return Ok(false),
				};

				let compute = ComputeV2 {
					key_hash,
					difficulty,
					pre_hash: *pre_hash,
					nonce: seal.nonce,
				};

				let pre_digest = match pre_digest {
					Some(pre_digest) => pre_digest,
					None => return Ok(false),
				};

				let author = match app::Public::decode(&mut &pre_digest[..]) {
					Ok(author) => author,
					Err(_) => return Ok(false),
				};

				if !compute.verify(&seal.signature, &author) {
					return Ok(false);
				}

				let (computed_seal, computed_work) =
					compute.seal_and_work(seal.signature.clone(), ComputeMode::Sync)?;

				if computed_seal != seal {
					return Ok(false);
				}

				if !is_valid_hash(&computed_work, difficulty) {
					return Ok(false);
				}

				Ok(true)
			}
		}
	}
}

#[derive(Debug)]
pub enum Error<B>
where
	B: sp_runtime::traits::Block,
{
	Consensus(crain_pow_consensus::Error<B>),
	Compute(compute::Error),
}

impl<B> From<crain_pow_consensus::Error<B>> for Error<B>
where
	B: sp_runtime::traits::Block,
{
	fn from(e: crain_pow_consensus::Error<B>) -> Self {
		Error::Consensus(e)
	}
}

impl<B> From<compute::Error> for Error<B>
where
	B: sp_runtime::traits::Block,
{
	fn from(e: compute::Error) -> Self {
		Error::Compute(e)
	}
}

pub struct Stats {
	_last_clear: Instant,
	_last_display: Instant,
	_round: u32,
}

impl Stats {
	pub fn new() -> Stats {
		Self {
			_last_clear: Instant::now(),
			_last_display: Instant::now(),
			_round: 0,
		}
	}
}

pub fn mine<B, C>(
	client: &C,
	keystore: &LocalKeystore,
	parent: &BlockId<B>,
	pre_hash: &H256,
	pre_digest: Option<&[u8]>,
	difficulty: Difficulty,
	round: u32,
	_stats: &Arc<Mutex<Stats>>,
) -> Result<Option<RawSeal>, Error<B>>
where
	B: BlockT<Hash = H256>,
	C: HeaderBackend<B> + AuxStore + ProvideRuntimeApi<B>,
	C::Api: DifficultyApi<B, Difficulty>,
{

	let version = RandomXAlgorithmVersion::V2;

	let mut rng = SmallRng::from_rng(&mut thread_rng()).map_err(|e| {
		crain_pow_consensus::Error::Environment(format!(
			"Initialize RNG failed for mining: {:?}",
			e
		))
	})?;
	let key_hash = key_hash(client, parent)?;

	let pre_digest = pre_digest.ok_or(crain_pow_consensus::Error::<B>::Other(
		"Unable to mine: pre-digest not set".to_string(),
	))?;

	let author = app::Public::decode(&mut &pre_digest[..]).map_err(|_| {
		crain_pow_consensus::Error::<B>::Other(
			"Unable to mine: author pre-digest decoding failed".to_string(),
		)
	})?;

	let pair = keystore
		.key_pair::<app::Pair>(&author)
		.map_err(|_| {
			crain_pow_consensus::Error::<B>::Other(
				"Unable to mine: fetch pair from author failed".to_string(),
			)
		})?
		.ok_or(crain_pow_consensus::Error::<B>::Other(
			"Unable to mine: key not found in keystore".to_string(),
		))?;

	let maybe_seal = match version {
		// TODO This branch will never fire
		RandomXAlgorithmVersion::V1 => compute::loop_raw(
			&key_hash,
			ComputeMode::Mining,
			|| {
				let nonce = H256::random_using(&mut rng);

				let compute = ComputeV1 {
					key_hash,
					difficulty,
					pre_hash: *pre_hash,
					nonce,
				};

				(compute.input().encode(), compute)
			},
			|work, compute| {
				if is_valid_hash(&work, compute.difficulty) {
					let seal = compute.seal();
					compute::Loop::Break(Some(seal.encode()))
				} else {
					compute::Loop::Continue
				}
			},
			round as usize,
		),
		RandomXAlgorithmVersion::V2 => compute::loop_raw(
			&key_hash,
			ComputeMode::Mining,
			|| {
				let nonce = H256::random_using(&mut rng);

				let compute = ComputeV2 {
					key_hash,
					difficulty,
					pre_hash: *pre_hash,
					nonce,
				};

				let signature = compute.sign(&pair);

				(
					compute.input(signature.clone()).encode(),
					(compute, signature),
				)
			},
			|work, (compute, signature)| {
				if is_valid_hash(&work, difficulty) {
					let seal = compute.seal(signature);
					compute::Loop::Break(Some(seal.encode()))
				} else {
					compute::Loop::Continue
				}
			},
			round as usize,
		),
	};


	Ok(maybe_seal?)
}
