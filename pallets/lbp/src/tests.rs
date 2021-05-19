use super::*;
pub use crate::mock::{
	run_to_block, Currency, Event as TestEvent, ExtBuilder, LBPPallet, Origin, System, Test, ACA, ALICE, BOB,
	DOT, ETH, HDX,
};
use crate::mock::{ACA_DOT_POOL_ID, HDX_DOT_POOL_ID, INITIAL_BALANCE, POOL_DEPOSIT};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::BadOrigin;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext = ExtBuilder::default().build();
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn predefined_test_ext() -> sp_io::TestExternalities {
	let mut ext = new_test_ext();
	ext.execute_with(|| {
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: ACA,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 90,
			},
			LBPAssetInfo{
				id: DOT,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 10,
			},
			(10u64, 20u64),
			WeightCurveType::Linear,
			true,
		));
	});
	ext
}

fn last_events(n: usize) -> Vec<TestEvent> {
	frame_system::Pallet::<Test>::events()
		.into_iter()
		.rev()
		.take(n)
		.rev()
		.map(|e| e.event)
		.collect()
}

fn expect_events(e: Vec<TestEvent>) {
	assert_eq!(last_events(e.len()), e);
}

// TODO: move me to the hydradx-math crate
#[test]
fn linear_weights_should_work() {
	let u32_cases = vec![
		(100u32, 200u32, 1_000u128, 2_000u128, 170u32, Ok(1_700), "Easy case"),
		(
			100u32,
			200u32,
			2_000u128,
			1_000u128,
			170u32,
			Ok(1_300),
			"Easy decreasing case",
		),
		(
			100u32,
			200u32,
			2_000u128,
			2_000u128,
			170u32,
			Ok(2_000),
			"Easy constant case",
		),
		(
			100u32,
			200u32,
			1_000u128,
			2_000u128,
			100u32,
			Ok(1_000),
			"Initial weight",
		),
		(
			100u32,
			200u32,
			2_000u128,
			1_000u128,
			100u32,
			Ok(2_000),
			"Initial decreasing weight",
		),
		(
			100u32,
			200u32,
			2_000u128,
			2_000u128,
			100u32,
			Ok(2_000),
			"Initial constant weight",
		),
		(100u32, 200u32, 1_000u128, 2_000u128, 200u32, Ok(2_000), "Final weight"),
		(
			100u32,
			200u32,
			2_000u128,
			1_000u128,
			200u32,
			Ok(1_000),
			"Final decreasing weight",
		),
		(
			100u32,
			200u32,
			2_000u128,
			2_000u128,
			200u32,
			Ok(2_000),
			"Final constant weight",
		),
		(
			200u32,
			100u32,
			1_000u128,
			2_000u128,
			170u32,
			Err(Overflow),
			"Invalid interval",
		),
		(
			100u32,
			100u32,
			1_000u128,
			2_000u128,
			100u32,
			Err(ZeroDuration),
			"Invalid interval",
		),
		(
			100u32,
			200u32,
			1_000u128,
			2_000u128,
			10u32,
			Err(Overflow),
			"Out of bound",
		),
		(
			100u32,
			200u32,
			1_000u128,
			2_000u128,
			210u32,
			Err(Overflow),
			"Out of bound",
		),
	];
	let u64_cases = vec![
		(100u64, 200u64, 1_000u128, 2_000u128, 170u64, Ok(1_700), "Easy case"),
		(
			100u64,
			u64::MAX,
			1_000u128,
			2_000u128,
			200u64,
			Err(Overflow),
			"Interval too long",
		),
	];

	for case in u32_cases {
		assert_eq!(
			crate::calculate_linear_weights(case.0, case.1, case.2, case.3, case.4),
			case.5,
			"{}",
			case.6
		);
	}
	for case in u64_cases {
		assert_eq!(
			crate::calculate_linear_weights(case.0, case.1, case.2, case.3, case.4),
			case.5,
			"{}",
			case.6
		);
	}
}

#[test]
fn weight_update_should_work() {
	new_test_ext().execute_with(|| {
		let asset_a = LBPAssetInfo{
			id: HDX,
			amount: 1,
			initial_weight: 20,
			final_weight: 80,
		};
		let asset_b = LBPAssetInfo{
			id: DOT,
			amount: 2,
			initial_weight: 80,
			final_weight: 20,
		};
		let asset_c = LBPAssetInfo{
			id: ACA,
			amount: 2,
			initial_weight: 80,
			final_weight: 20,
		};
		let duration = (10u64, 19u64);

		let mut linear_pool = Pool::new(asset_a, asset_b, duration, WeightCurveType::Linear, false);
		let mut constant_pool = Pool::new(asset_a, asset_c, duration, WeightCurveType::Constant, false);

		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			asset_a,
			asset_b,
			duration,
			WeightCurveType::Linear,
			false
		));
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			asset_a,
			asset_c,
			duration,
			WeightCurveType::Constant,
			false
		));

		System::set_block_number(13);

		assert_ok!(LBPPallet::update_weights(&mut linear_pool));
		assert_ok!(LBPPallet::update_weights(&mut constant_pool));

		assert_eq!(linear_pool.last_weight_update, 13);
		assert_eq!(constant_pool.last_weight_update, 13);

		assert_eq!(linear_pool.last_weights, ((HDX, 40u128), (DOT, 60u128)));
		assert_eq!(constant_pool.last_weights, ((HDX, 20u128), (ACA, 80u128)));

		// call update again in the same block, data should be the same
		assert_ok!(LBPPallet::update_weights(&mut linear_pool));
		assert_ok!(LBPPallet::update_weights(&mut constant_pool));

		assert_eq!(linear_pool.last_weight_update, 13);
		assert_eq!(constant_pool.last_weight_update, 13);

		assert_eq!(linear_pool.last_weights, ((HDX, 40u128), (DOT, 60u128)));
		assert_eq!(constant_pool.last_weights, ((HDX, 20u128), (ACA, 80u128)));
	});
}

#[test]
fn validate_pool_data_should_work() {
	new_test_ext().execute_with(|| {
		let pool_data = Pool {
			start: 10u64,
			end: 20u64,
			initial_weights: ((1, 20), (2, 80)),
			final_weights: ((1, 90), (2, 10)),
			last_weight_update: 0u64,
			last_weights: ((1, 20), (2, 80)),
			curve: WeightCurveType::Linear,
			pausable: true,
			paused: false,
		};
		assert_ok!(LBPPallet::validate_pool_data(&pool_data));

		let pool_data = Pool {
			start: 0u64,
			end: 0u64,
			initial_weights: ((1, 20), (2, 80)),
			final_weights: ((1, 90), (2, 10)),
			last_weight_update: 0u64,
			last_weights: ((1, 20), (2, 80)),
			curve: WeightCurveType::Linear,
			pausable: true,
			paused: false,
		};
		assert_ok!(LBPPallet::validate_pool_data(&pool_data));

		let pool_data = Pool {
			start: 10u64,
			end: 2u64,
			initial_weights: ((1, 20), (2, 80)),
			final_weights: ((1, 90), (2, 10)),
			last_weight_update: 0u64,
			last_weights: ((1, 20), (2, 80)),
			curve: WeightCurveType::Linear,
			pausable: true,
			paused: false,
		};
		assert_noop!(
			LBPPallet::validate_pool_data(&pool_data),
			Error::<Test>::InvalidBlockNumber
		);

		let pool_data = Pool {
			start: 10u64,
			end: 11u64 + u32::MAX as u64,
			initial_weights: ((1, 20), (2, 80)),
			final_weights: ((1, 90), (2, 10)),
			last_weight_update: 0u64,
			last_weights: ((1, 20), (2, 80)),
			curve: WeightCurveType::Linear,
			pausable: true,
			paused: false,
		};
		assert_noop!(
			LBPPallet::validate_pool_data(&pool_data),
			Error::<Test>::MaxSaleDurationExceeded
		);
	});
}

#[test]
fn create_pool_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: ACA,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 90,
			},
			LBPAssetInfo{
				id: DOT,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 10,
			},
			(10u64, 20u64),
			WeightCurveType::Linear,
			true,
		));

		assert_eq!(Currency::free_balance(ACA, &ACA_DOT_POOL_ID), 1_000_000_000);
		assert_eq!(Currency::free_balance(DOT, &ACA_DOT_POOL_ID), 2_000_000_000);
		assert_eq!(
			Currency::free_balance(ACA, &ALICE),
			INITIAL_BALANCE.saturating_sub(1_000_000_000)
		);
		assert_eq!(
			Currency::free_balance(DOT, &ALICE),
			INITIAL_BALANCE.saturating_sub(2_000_000_000)
		);
		assert_eq!(Currency::reserved_balance(HDX, &ALICE), POOL_DEPOSIT);
		assert_eq!(
			Currency::free_balance(HDX, &ALICE),
			INITIAL_BALANCE.saturating_sub(POOL_DEPOSIT)
		);
		assert_eq!(LBPPallet::pool_deposit(&ACA_DOT_POOL_ID), POOL_DEPOSIT);

		assert_eq!(LBPPallet::get_pool_assets(&ACA_DOT_POOL_ID).unwrap(), vec![ACA, DOT]);

		// verify that `last_weight_update`, `last_weights` and `paused` fields are correctly initialized
		let updated_pool_data = LBPPallet::pool_data(ACA_DOT_POOL_ID);
		assert_eq!(updated_pool_data.last_weight_update, 0);
		assert_eq!(updated_pool_data.last_weights, ((ACA, 20), (DOT, 80)));
		assert_eq!(updated_pool_data.paused, false);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into()
		]);
	});
}

#[test]
fn create_pool_from_basic_origin_should_not_work() {
	new_test_ext().execute_with(|| {
		// only CreatePoolOrigin is allowed to create new pools
		assert_noop!(LBPPallet::create_pool(
			Origin::signed(ALICE),
			ALICE,
			LBPAssetInfo{
				id: HDX,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 90,
			},
			LBPAssetInfo{
				id: DOT,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 10,
			},
			(10u64, 20u64),
			WeightCurveType::Linear,
			true,
		),
		BadOrigin);
	});
}

#[test]
fn create_same_pool_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: ACA,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 90,
			},
			LBPAssetInfo{
				id: DOT,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 10,
			},
			(10u64, 20u64),
			WeightCurveType::Linear,
			true,
		));

		assert_noop!(
			LBPPallet::create_pool(
				Origin::root(),
				ALICE,
				LBPAssetInfo{
					id: ACA,
					amount: 10_000_000_000,
					initial_weight: 30,
					final_weight: 70,
				},
				LBPAssetInfo{
					id: DOT,
					amount: 20_000_000_000,
					initial_weight: 70,
					final_weight: 30,
				},
				(100u64, 200u64),
				WeightCurveType::Linear,
				true,
			),
			Error::<Test>::PoolAlreadyExists
		);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into()
		]);
	});
}

#[test]
fn create_pool_invalid_data_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LBPPallet::create_pool(
				Origin::root(),
				ALICE,
				LBPAssetInfo{
					id: ACA,
					amount: 1_000_000_000,
					initial_weight: 20,
					final_weight: 90,
				},
				LBPAssetInfo{
					id: DOT,
					amount: 2_000_000_000,
					initial_weight: 80,
					final_weight: 10,
				},
			(20u64, 10u64),	// reversed interval, the end precedes the beginning
			WeightCurveType::Linear,
			true,
			),
			Error::<Test>::InvalidBlockNumber
		);
	});
}

#[test]
fn update_pool_data_should_work() {
	predefined_test_ext().execute_with(|| {
		// update starting block and final weights
		assert_ok!(LBPPallet::update_pool_data(
			Origin::signed(ALICE),
			ACA_DOT_POOL_ID,
			Some(15),
			None,
			Some(((1, 10), (2, 90))),
			None,
		));

		// verify changes
		let updated_pool_data = LBPPallet::pool_data(ACA_DOT_POOL_ID);
		assert_eq!(updated_pool_data.start, 15);
		assert_eq!(updated_pool_data.end, 20);

		expect_events(vec![Event::PoolUpdated(ALICE, ACA_DOT_POOL_ID).into()]);
	});
}

#[test]
fn update_pool_data_for_running_lbp_should_not_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(16);

		// update starting block and final weights
		assert_noop!(
			LBPPallet::update_pool_data(
				Origin::signed(ALICE),
				ACA_DOT_POOL_ID,
				Some(15),
				None,
				Some(((1, 10), (2, 90))),
				None,
			),
			Error::<Test>::SaleStarted
		);

		let updated_pool_data = LBPPallet::pool_data(ACA_DOT_POOL_ID);
		assert_eq!(updated_pool_data.start, 10);
		assert_eq!(updated_pool_data.end, 20);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into()
		]);
	});
}

#[test]
fn pause_pool_should_work() {
	predefined_test_ext().execute_with(|| {
		assert_ok!(LBPPallet::pause_pool(Origin::signed(ALICE), ACA_DOT_POOL_ID));

		let paused_pool = LBPPallet::pool_data(ACA_DOT_POOL_ID);
		assert_eq!(
			paused_pool,
			Pool {
				start: 10u64,
				end: 20u64,
				initial_weights: ((ACA, 20), (DOT, 80)),
				final_weights: ((ACA, 90), (DOT, 10)),
				last_weight_update: 0u64,
				last_weights: ((ACA, 20), (DOT, 80)),
				curve: WeightCurveType::Linear,
				pausable: true,
				paused: true
			}
		);

		expect_events(vec![Event::Paused(ALICE, ACA_DOT_POOL_ID).into()]);
	});
}

#[test]
fn pause_pool_should_not_work() {
	predefined_test_ext().execute_with(|| {
		//user is not pool owner
		let not_owner = BOB;
		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(not_owner), ACA_DOT_POOL_ID),
			Error::<Test>::NotOwner
		);

		//pool is not found
		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(ALICE), 24568),
			Error::<Test>::PoolNotFound
		);

		//pool is not puasable
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			BOB,
			LBPAssetInfo{
				id: ACA,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: ETH,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			false,
		));

		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(BOB), 2_004_000),
			Error::<Test>::PoolIsNotPausable
		);

		//pool is already paused
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			BOB,
			LBPAssetInfo{
				id: DOT,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: ETH,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			true,
		));

		// pause the pool
		assert_ok!(LBPPallet::pause_pool(Origin::signed(BOB), 3_004_000));
		// pool is already paused
		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(BOB), 3_004_000),
			Error::<Test>::CannotPausePausedPool
		);

		//pool ended or ending in current block
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: DOT,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: HDX,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			true,
		));

		run_to_block(400);
		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID),
			Error::<Test>::CannotPauseEndedPool
		);

		run_to_block(500);
		assert_noop!(
			LBPPallet::pause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID),
			Error::<Test>::CannotPauseEndedPool
		);
	});
}

#[test]
fn unpause_pool_should_work() {
	predefined_test_ext().execute_with(|| {
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: DOT,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: HDX,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			true,
		));

		// pause the pool before trying to unpause it
		assert_ok!(LBPPallet::pause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID,));
		assert_ok!(LBPPallet::unpause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID,));

		let unpaused_pool = LBPPallet::pool_data(HDX_DOT_POOL_ID);
		assert_eq!(
			unpaused_pool,
			Pool {
				start: 200_u64,
				end: 400_u64,
				initial_weights: ((DOT, 20), (HDX, 80)),
				final_weights: ((DOT, 40), (HDX, 60)),
				last_weight_update: 0u64,
				last_weights: ((DOT, 20), (HDX, 80)),
				curve: WeightCurveType::Linear,
				pausable: true,
				paused: false
			}
		);

		expect_events(vec![
			Event::Paused(ALICE, HDX_DOT_POOL_ID).into(),
			Event::Unpaused(ALICE, HDX_DOT_POOL_ID).into(),
		]);
	});
}

#[test]
fn unpause_pool_should_not_work() {
	predefined_test_ext().execute_with(|| {
		//user is not pool owner
		let not_owner = BOB;
		assert_noop!(
			LBPPallet::unpause_pool(Origin::signed(not_owner), ACA_DOT_POOL_ID),
			Error::<Test>::NotOwner
		);

		//pool is not found
		assert_noop!(
			LBPPallet::unpause_pool(Origin::signed(ALICE), 24568),
			Error::<Test>::PoolNotFound
		);

		//pool is not puased
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			BOB,
			LBPAssetInfo{
				id: ACA,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: ETH,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			false,
		));

		assert_noop!(
			LBPPallet::unpause_pool(Origin::signed(BOB), 2_004_000),
			Error::<Test>::PoolIsNotPaused
		);

		//pooled ended or ending in current block
		assert_ok!(LBPPallet::create_pool(
			Origin::root(),
			ALICE,
			LBPAssetInfo{
				id: DOT,
				amount: 1_000_000_000,
				initial_weight: 20,
				final_weight: 40,
			},
			LBPAssetInfo{
				id: HDX,
				amount: 2_000_000_000,
				initial_weight: 80,
				final_weight: 60,
			},
			(200u64, 400u64),
			WeightCurveType::Linear,
			true,
		));

		// pause the pool before trying to unpause it
		assert_ok!(LBPPallet::pause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID,));

		run_to_block(400);
		assert_noop!(
			LBPPallet::unpause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID),
			Error::<Test>::CannotUnpauseEndedPool
		);

		run_to_block(500);
		assert_noop!(
			LBPPallet::unpause_pool(Origin::signed(ALICE), HDX_DOT_POOL_ID),
			Error::<Test>::CannotUnpauseEndedPool
		);
	});
}

#[test]
fn add_liquidity_should_work() {
	predefined_test_ext().execute_with(|| {
		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		let added_a = 10_000_000_000;
		let added_b = 20_000_000_000;

		assert_ok!(LBPPallet::add_liquidity(
			Origin::signed(ALICE),
			ACA_DOT_POOL_ID,
			added_a,
			added_b,
		));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before.saturating_add(added_a));
		assert_eq!(balance_b_after, balance_b_before.saturating_add(added_b));

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before.saturating_sub(added_a));
		assert_eq!(user_balance_b_after, user_balance_b_before.saturating_sub(added_b));

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into(),
			Event::LiquidityAdded(ACA_DOT_POOL_ID, ACA, DOT, added_a, added_b).into(),
		]);

		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_ok!(LBPPallet::add_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, added_a, 0,));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_eq!(balance_a_after, balance_a_before.saturating_add(added_a));
		assert_eq!(balance_b_after, balance_b_before);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into(),
			Event::LiquidityAdded(ACA_DOT_POOL_ID, ACA, DOT, added_a, added_b).into(),
			Event::LiquidityAdded(ACA_DOT_POOL_ID, ACA, DOT, added_a, 0).into(),
		]);
	});
}

#[test]
fn add_zero_liquidity_should_not_work() {
	predefined_test_ext().execute_with(|| {
		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::add_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 0, 0,),
			Error::<Test>::CannotAddZeroLiquidity
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);
		assert_eq!(user_balance_b_after, user_balance_b_before);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}

#[test]
fn add_liquidity_insufficient_balance_should_not_work() {
	predefined_test_ext().execute_with(|| {
		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::add_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, u128::MAX, 0,),
			Error::<Test>::InsufficientAssetBalance
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);
	});
}

#[test]
fn add_liquidity_after_sale_started_should_not_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(15);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::add_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 1_000, 1_000,),
			Error::<Test>::SaleStarted
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);
		assert_eq!(user_balance_b_after, user_balance_b_before);

		// sale ended at the block number 20
		System::set_block_number(30);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::add_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 1_000, 1_000,),
			Error::<Test>::SaleStarted
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);
		assert_eq!(user_balance_b_after, user_balance_b_before);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}

#[test]
fn remove_liquidity_should_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(5);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_ok!(LBPPallet::remove_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 1_000, 0,));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before.saturating_sub(1_000));
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before.saturating_add(1_000));

		System::set_block_number(30);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		let removed_a = 10_000_000;
		let removed_b = 20_000_000;

		assert_ok!(LBPPallet::remove_liquidity(
			Origin::signed(ALICE),
			ACA_DOT_POOL_ID,
			removed_a,
			removed_b,
		));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before.saturating_sub(removed_a));
		assert_eq!(balance_b_after, balance_b_before.saturating_sub(removed_b));

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before.saturating_add(removed_a));
		assert_eq!(user_balance_b_after, user_balance_b_before.saturating_add(removed_b));

		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_ok!(LBPPallet::remove_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, removed_a, 0,));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_eq!(balance_a_after, balance_a_before.saturating_sub(removed_a));
		assert_eq!(balance_b_after, balance_b_before);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into(),
			Event::LiquidityRemoved(ACA_DOT_POOL_ID, ACA, DOT, 1_000, 0).into(),
			Event::LiquidityRemoved(ACA_DOT_POOL_ID, ACA, DOT, removed_a, removed_b).into(),
			Event::LiquidityRemoved(ACA_DOT_POOL_ID, ACA, DOT, removed_a, 0).into(),
		]);
	});
}

#[test]
fn remove_zero_liquidity_should_not_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(30);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::remove_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 0, 0,),
			Error::<Test>::CannotRemoveZeroLiquidity
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);
		assert_eq!(user_balance_b_after, user_balance_b_before);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}

#[test]
fn remove_liquidity_insufficient_reserve_should_not_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(30);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::remove_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, u128::MAX, 0,),
			Error::<Test>::LiquidityUnderflow
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}

#[test]
fn remove_liquidity_during_sale_should_not_work() {
	predefined_test_ext().execute_with(|| {
		// sale started at the block number 10
		System::set_block_number(15);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::remove_liquidity(Origin::signed(ALICE), ACA_DOT_POOL_ID, 1_000, 0,),
			Error::<Test>::SaleNotEnded
		);

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, balance_a_before);
		assert_eq!(balance_b_after, balance_b_before);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		assert_eq!(user_balance_a_after, user_balance_a_before);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}

#[test]
fn destroy_pool_should_work() {
	predefined_test_ext().execute_with(|| {
		System::set_block_number(21);

		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let user_balance_hdx_before = Currency::reserved_balance(HDX, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_ok!(LBPPallet::destroy_pool(Origin::signed(ALICE), ACA_DOT_POOL_ID,));

		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);
		assert_eq!(balance_a_after, 0);
		assert_eq!(balance_b_after, 0);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		assert_eq!(
			user_balance_a_after,
			user_balance_a_before.saturating_add(balance_a_before)
		);

		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		assert_eq!(
			user_balance_b_after,
			user_balance_b_before.saturating_add(balance_b_before)
		);

		let user_balance_hdx_after = Currency::reserved_balance(HDX, &ALICE);
		assert_eq!(
			user_balance_hdx_after,
			user_balance_hdx_before.saturating_sub(POOL_DEPOSIT)
		);

		expect_events(vec![
			Event::PoolCreated(ALICE, ACA, DOT, 1_000_000_000, 2_000_000_000).into(),
			frame_system::Event::KilledAccount(ACA_DOT_POOL_ID).into(),
			Event::PoolDestroyed(ACA_DOT_POOL_ID, ACA, DOT, balance_a_before, balance_b_before).into(),
		]);
	});
}

#[test]
fn destroy_not_finalized_pool_should_not_work() {
	predefined_test_ext().execute_with(|| {
		let user_balance_a_before = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_before = Currency::free_balance(DOT, &ALICE);
		let user_balance_hdx_before = Currency::reserved_balance(HDX, &ALICE);
		let (balance_a_before, balance_b_before) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_noop!(
			LBPPallet::destroy_pool(Origin::signed(ALICE), ACA_DOT_POOL_ID,),
			Error::<Test>::SaleNotEnded
		);

		let user_balance_a_after = Currency::free_balance(ACA, &ALICE);
		let user_balance_b_after = Currency::free_balance(DOT, &ALICE);
		let user_balance_hdx_after = Currency::reserved_balance(HDX, &ALICE);
		let (balance_a_after, balance_b_after) = LBPPallet::pool_balances(ACA_DOT_POOL_ID);

		assert_eq!(balance_a_before, balance_a_after);
		assert_eq!(balance_b_before, balance_b_after);
		assert_eq!(user_balance_a_before, user_balance_a_after);
		assert_eq!(user_balance_b_before, user_balance_b_after);
		assert_eq!(user_balance_hdx_before, user_balance_hdx_after);

		expect_events(vec![Event::PoolCreated(
			ALICE,
			ACA,
			DOT,
			1_000_000_000,
			2_000_000_000,
		)
		.into()]);
	});
}
