// This file is part of Basilisk-node.

// Copyright (C) 2020-2021  Intergalactic, Limited (GIB).
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unnecessary_wraps)]

mod mock;

use pallet_liquidity_mining::Pallet as LiquidityMining;

use frame_benchmarking::{account, benchmarks};
use frame_system::{Pallet as System, RawOrigin};

use frame_support::dispatch;
use orml_traits::MultiCurrency;
use primitives::{asset::AssetPair, AssetId, Balance, Price};
use sp_arithmetic::FixedU128;
use sp_arithmetic::Permill;
use sp_std::convert::From;

use primitives::constants::currency::NATIVE_EXISTENTIAL_DEPOSIT;

use pallet_xyk as xykpool;

const SEED: u32 = 0;

const BSX: AssetId = 0;
const KSM: AssetId = 1;

pub trait Config: pallet_liquidity_mining::Config + pallet_xyk::Config {}

pub struct Pallet<T: Config>(LiquidityMining<T>);

const INITIAL_BALANCE: Balance = 100_000_000_000_000_000;

fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
	let caller: T::AccountId = account(name, index, SEED);

	<T as pallet_liquidity_mining::Config>::MultiCurrency::deposit(
		BSX.into(),
		&caller,
		INITIAL_BALANCE * NATIVE_EXISTENTIAL_DEPOSIT,
	)
	.unwrap();

	<T as pallet_liquidity_mining::Config>::MultiCurrency::deposit(
		KSM.into(),
		&caller,
		INITIAL_BALANCE * NATIVE_EXISTENTIAL_DEPOSIT,
	)
	.unwrap();

	caller
}

fn initialize_pool<T: Config>(
	caller: T::AccountId,
	asset_a: AssetId,
	asset_b: AssetId,
	amount: Balance,
	price: Price,
) -> dispatch::DispatchResult {
	xykpool::Pallet::<T>::create_pool(RawOrigin::Signed(caller).into(), asset_a, asset_b, amount, price)?;

	Ok(())
}

fn init_farm<T: Config>(
	total_rewards: Balance,
	owner: T::AccountId,
	yield_per_period: Permill,
) -> dispatch::DispatchResult {
	LiquidityMining::<T>::create_farm(
		RawOrigin::Root.into(),
		total_rewards * NATIVE_EXISTENTIAL_DEPOSIT,
		T::BlockNumber::from(1_000_000_u32),
		T::BlockNumber::from(1_u32),
		BSX.into(),
		BSX.into(),
		owner,
		yield_per_period,
	)?;

	Ok(())
}

fn xyk_add_liquidity<T: Config>(
	caller: T::AccountId,
	assets: AssetPair,
	amount_a: Balance,
	amount_b_max: Balance,
) -> dispatch::DispatchResult {
	xykpool::Pallet::<T>::add_liquidity(
		RawOrigin::Signed(caller).into(),
		assets.asset_in,
		assets.asset_out,
		amount_a,
		amount_b_max,
	)?;

	Ok(())
}

fn lm_deposit_shares<T: Config>(caller: T::AccountId, assets: AssetPair, amount: Balance) -> dispatch::DispatchResult {
	LiquidityMining::<T>::deposit_shares(RawOrigin::Signed(caller).into(), 1, assets, amount)?;

	Ok(())
}

fn lm_add_liquidity_pool<T: Config>(
	caller: T::AccountId,
	assets: AssetPair,
	multiplier: FixedU128,
) -> dispatch::DispatchResult {
	LiquidityMining::<T>::add_liquidity_pool(
		RawOrigin::Signed(caller).into(),
		1,
		assets,
		multiplier,
		Some(pallet_liquidity_mining::LoyaltyCurve::default()),
	)?;

	Ok(())
}

fn set_block_number<T: Config>(block: u32) {
	System::<T>::set_block_number(block.into());
}

benchmarks! {
	create_farm {
		let caller = funded_account::<T>("caller", 0);
	}: { LiquidityMining::<T>::create_farm(RawOrigin::Root.into(), 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, T::BlockNumber::from(1_000_000_u32), T::BlockNumber::from(1_u32), BSX.into(), BSX.into(), caller.clone(), Permill::from_percent(20))? }
	verify {
		assert!(LiquidityMining::<T>::global_pool(1).is_some());
	}

	destroy_farm {
		let caller = funded_account::<T>("caller", 0);

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		assert!(LiquidityMining::<T>::global_pool(1).is_some());

		//there can't be undistributed rewards
		LiquidityMining::<T>::withdraw_undistributed_rewards(RawOrigin::Signed(caller.clone()).into(), 1)?;
	}: { LiquidityMining::<T>::destroy_farm(RawOrigin::Signed(caller.clone()).into(), 1)? }
	verify {
		assert!(LiquidityMining::<T>::global_pool(1).is_none());
	}

	withdraw_undistributed_rewards {
		let caller = funded_account::<T>("caller", 0);

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);
	}: { LiquidityMining::<T>::withdraw_undistributed_rewards(RawOrigin::Signed(caller.clone()).into(), 1)? }
	verify {
		assert_eq!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account), 0);
		assert_eq!(T::MultiCurrency::free_balance(BSX.into(), &caller), INITIAL_BALANCE * NATIVE_EXISTENTIAL_DEPOSIT);
	}

	add_liquidity_pool {
		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

	}: {
		LiquidityMining::<T>::add_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets, FixedU128::from(50_000_u128), Some(pallet_liquidity_mining::LoyaltyCurve::default()))?
	}
	verify {
		assert_eq!(LiquidityMining::<T>::global_pool(1).unwrap().liq_pools_count, 1);

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).is_some());

		assert_eq!(LiquidityMining::<T>::liq_pool_meta(2).unwrap(), (assets, 0, 1));
	}

	update_liquidity_pool {
		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		LiquidityMining::<T>::add_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets, FixedU128::from(50_000_u128), Some(pallet_liquidity_mining::LoyaltyCurve::default()))?;

		assert_eq!(LiquidityMining::<T>::global_pool(1).unwrap().liq_pools_count, 1);

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().multiplier, FixedU128::from(50_000_u128));
	}: {
		LiquidityMining::<T>::update_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets, FixedU128::from(10_000_u128))?
	}
	verify {
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().multiplier, FixedU128::from(10_000_u128));
	}

	cancel_liquidity_pool {
		//init nft class for liq. mining
		pallet_liquidity_mining::migration::init_nft_class::<T>();

		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);
		let liq_provider = funded_account::<T>("liq_provider", 2);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller.clone(), assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		xyk_add_liquidity::<T>(liq_provider.clone(), assets, 10_000, 1_000_000_000)?;

		lm_deposit_shares::<T>(liq_provider, assets, 10_000)?;

		set_block_number::<T>(200_000);

		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().updated_at, 0_u32.into());
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().updated_at, 0_u32.into());
	}: {
		LiquidityMining::<T>::cancel_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets)?
	}
	verify {
		assert!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().updated_at, 200_000_u32.into());
	}

	remove_liquidity_pool {
		//init nft class for liq. mining
		pallet_liquidity_mining::migration::init_nft_class::<T>();

		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);
		let liq_provider = funded_account::<T>("liq_provider", 2);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller.clone(), assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		xyk_add_liquidity::<T>(liq_provider.clone(), assets, 10_000, 1_000_000_000)?;

		lm_deposit_shares::<T>(liq_provider.clone(), assets, 10_000)?;

		set_block_number::<T>(200_000);

		LiquidityMining::<T>::withdraw_shares(
			RawOrigin::Signed(liq_provider).into(),
			4_294_967_298_u128
		)?;

		LiquidityMining::<T>::cancel_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets)?;

		assert!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);
	}: {
		LiquidityMining::<T>::remove_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets)?
	}
	verify {
		assert!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).is_none());
		assert_eq!(LiquidityMining::<T>::global_pool(1).unwrap().liq_pools_count, 0);
		assert!(LiquidityMining::<T>::liq_pool_meta(1).is_none());
	}

	deposit_shares {
		//init nft class for liq. mining
		pallet_liquidity_mining::migration::init_nft_class::<T>();

		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);
		let liq_provider = funded_account::<T>("liq_provider", 2);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller, assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		xyk_add_liquidity::<T>(liq_provider.clone(), assets, 10_000, 1_000_000_000)?;

		set_block_number::<T>(200_000);

		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().updated_at, 0_u32.into());
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().updated_at, 0_u32.into());

		assert!(LiquidityMining::<T>::deposit(
			4_294_967_298_u128
		).is_none());

	}: {
		LiquidityMining::<T>::deposit_shares(RawOrigin::Signed(liq_provider.clone()).into(), 1, assets, 10_000)?
	}
	verify {
		assert!(LiquidityMining::<T>::deposit(
			4_294_967_298_u128
		).is_some());
	}

	claim_rewards {
		//init nft class for liq. mining
		pallet_liquidity_mining::migration::init_nft_class::<T>();

		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);
		let liq_provider = funded_account::<T>("liq_provider", 2);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(INITIAL_BALANCE, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert_eq!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account), INITIAL_BALANCE * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller, assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		xyk_add_liquidity::<T>(liq_provider.clone(), assets, 10_000, 1_000_000_000)?;

		set_block_number::<T>(200_000);

		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().updated_at, 0_u32.into());
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().updated_at, 0_u32.into());

		LiquidityMining::<T>::deposit_shares(RawOrigin::Signed(liq_provider.clone()).into(), 1, assets, 10_000)?;

		assert!(LiquidityMining::<T>::deposit(4_294_967_298_u128).is_some());

		set_block_number::<T>(400_000);

		let liq_provider_bsx_balance = T::MultiCurrency::free_balance(BSX.into(), &liq_provider);
	}: {
		LiquidityMining::<T>::claim_rewards(RawOrigin::Signed(liq_provider.clone()).into(), 4_294_967_298_u128)?
	}
	verify {
		assert!(T::MultiCurrency::free_balance(BSX.into(), &liq_provider).gt(&liq_provider_bsx_balance));
	}

	withdraw_shares {
		//init nft class for liq. mining
		pallet_liquidity_mining::migration::init_nft_class::<T>();

		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);
		let liq_provider = funded_account::<T>("liq_provider", 2);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(INITIAL_BALANCE, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert_eq!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account), INITIAL_BALANCE * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller, assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		xyk_add_liquidity::<T>(liq_provider.clone(), assets, 10_000, 1_000_000_000)?;

		set_block_number::<T>(200_000);

		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().updated_at, 0_u32.into());
		assert_eq!(LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().updated_at, 0_u32.into());

		LiquidityMining::<T>::deposit_shares(RawOrigin::Signed(liq_provider.clone()).into(), 1, assets, 10_000)?;

		assert!(LiquidityMining::<T>::deposit(4_294_967_298_u128).is_some());

		set_block_number::<T>(400_000);

		let liq_provider_bsx_balance = T::MultiCurrency::free_balance(BSX.into(), &liq_provider);
	}: {
		LiquidityMining::<T>::withdraw_shares(RawOrigin::Signed(liq_provider.clone()).into(), 4_294_967_298_u128)?
	}
	verify {
		assert!(T::MultiCurrency::free_balance(BSX.into(), &liq_provider).gt(&liq_provider_bsx_balance));
		assert!(LiquidityMining::<T>::deposit(4_294_967_298_u128).is_none());
	}

	//NOTE: This is same no matter if `update_global_pool()` is called because `GlobalPool`will be
	//read/written either way.
	resume_liquidity_pool {
		let caller = funded_account::<T>("caller", 0);
		let xyk_caller = funded_account::<T>("xyk_caller", 1);

		initialize_pool::<T>(xyk_caller, BSX, KSM, 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT, Price::from(10))?;

		init_farm::<T>(1_000_000, caller.clone(), Permill::from_percent(20))?;

		let global_pool_account = LiquidityMining::<T>::pool_account_id(1).unwrap();
		assert!(T::MultiCurrency::free_balance(BSX.into(), &global_pool_account) == 1_000_000 * NATIVE_EXISTENTIAL_DEPOSIT);

		let assets = AssetPair {
			asset_in: BSX,
			asset_out: KSM,
		};

		lm_add_liquidity_pool::<T>(caller.clone(), assets, FixedU128::from(50_000_u128))?;

		let xyk_id = xykpool::Pallet::<T>::pair_account_from_assets(assets.asset_in, assets.asset_out);
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);

		LiquidityMining::<T>::cancel_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets)?;

		assert!(LiquidityMining::<T>::liquidity_pool(1, xyk_id.clone()).unwrap().canceled);
	}: {
		LiquidityMining::<T>::resume_liquidity_pool(RawOrigin::Signed(caller.clone()).into(), 1, assets, FixedU128::from(12_452))?
	}
	verify {
		assert!(!LiquidityMining::<T>::liquidity_pool(1, xyk_id).unwrap().canceled);
	}
}

#[cfg(test)]
mod tests {
	use super::Pallet;
	use frame_benchmarking::impl_benchmark_test_suite;

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
