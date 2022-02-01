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

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

#![allow(clippy::all)]

use crate::client::{Client, RuntimeApiCollection};
use common_runtime::Block;
use cumulus_client_consensus_aura::{AuraConsensus, BuildAuraConsensusParams, SlotProportion};
use cumulus_client_consensus_common::ParachainConsensus;
use cumulus_client_network::BlockAnnounceValidator;
use cumulus_client_service::{
	prepare_node_config, start_collator, start_full_node, StartCollatorParams, StartFullNodeParams,
};
use cumulus_primitives_core::{CollectCollationInfo, ParaId};
use cumulus_relay_chain_interface::RelayChainInterface;
use cumulus_relay_chain_local::build_relay_chain_interface;
use sc_client_api::ExecutorProvider;
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch, NativeVersion};
use sc_network::NetworkService;
use sc_service::{ChainSpec, Configuration, PartialComponents, Role, TFullBackend, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_api::ConstructRuntimeApi;
use sp_consensus::SlotData;
use sp_keystore::SyncCryptoStorePtr;
use sp_runtime::traits::BlakeTwo256;
use sp_trie::PrefixedMemoryDB;
use std::{sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;
type Hash = sp_core::H256;

// native executor instance.
pub struct BasiliskExecutorDispatch;
impl sc_executor::NativeExecutionDispatch for BasiliskExecutorDispatch {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		basilisk_runtime::api::dispatch(method, data)
	}

	fn native_version() -> NativeVersion {
		basilisk_runtime::native_version()
	}
}

// native testing executor instance.
pub struct TestingBasiliskExecutorDispatch;
impl sc_executor::NativeExecutionDispatch for TestingBasiliskExecutorDispatch {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		testing_basilisk_runtime::api::dispatch(method, data)
	}

	fn native_version() -> NativeVersion {
		testing_basilisk_runtime::native_version()
	}
}

pub type FullBackend = TFullBackend<Block>;
pub type FullClient<RuntimeApi, ExecutorDispatch> =
	TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;

/// Can be called for a `Configuration` to check what node it belongs to.
pub trait IdentifyVariant {
	/// Returns if this is a configuration for the `Basilisk` node.
	fn is_basilisk_runtime(&self) -> bool;
	/// Returns if this is a configuration for the `Testing Basilisk` node.
	fn is_testing_runtime(&self) -> bool;
}

impl IdentifyVariant for Box<dyn ChainSpec> {
	fn is_basilisk_runtime(&self) -> bool {
		self.name().to_lowercase().starts_with("basilisk") || self.name().to_lowercase().starts_with("bsx")
	}
	fn is_testing_runtime(&self) -> bool {
		self.name().to_lowercase().starts_with("test")
	}
}

/// Build the import queue for the parachain runtime.
pub fn parachain_build_import_queue<RuntimeApi, Executor>(
	client: Arc<FullClient<RuntimeApi, Executor>>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, Executor>>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
	RuntimeApi::RuntimeApi: sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId>,
	Executor: NativeExecutionDispatch + 'static,
{
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _, _>(
		cumulus_client_consensus_aura::ImportQueueParams {
			block_import: client.clone(),
			client: client.clone(),
			create_inherent_data_providers: move |_, _| async move {
				let time = sp_timestamp::InherentDataProvider::from_system_time();

				let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_duration(
					*time,
					slot_duration.slot_duration(),
				);

				Ok((time, slot))
			},
			registry: config.prometheus_registry().clone(),
			can_author_with: sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
			spawner: &task_manager.spawn_essential_handle(),
			telemetry,
		},
	)
	.map_err(Into::into)
}

pub fn new_partial(
	mut config: &mut Configuration,
) -> Result<
	(
		Arc<Client>,
		Arc<FullBackend>,
		sc_consensus::import_queue::BasicQueue<Block, PrefixedMemoryDB<BlakeTwo256>>,
		TaskManager,
	),
	sc_service::Error,
> {
	config.keystore = sc_service::config::KeystoreConfig::InMemory;
	if config.chain_spec.is_testing_runtime() {
		let sc_service::PartialComponents {
			client,
			backend,
			import_queue,
			task_manager,
			..
		} = new_partial_impl::<testing_basilisk_runtime::RuntimeApi, TestingBasiliskExecutorDispatch>(config)?;
		Ok((
			Arc::new(Client::TestingBasilisk(client)),
			backend,
			import_queue,
			task_manager,
		))
	} else {
		let sc_service::PartialComponents {
			client,
			backend,
			import_queue,
			task_manager,
			..
		} = new_partial_impl::<basilisk_runtime::RuntimeApi, BasiliskExecutorDispatch>(config)?;
		Ok((Arc::new(Client::Basilisk(client)), backend, import_queue, task_manager))
	}
}

pub fn new_partial_impl<RuntimeApi, Executor>(
	config: &Configuration,
) -> Result<
	PartialComponents<
		FullClient<RuntimeApi, Executor>,
		FullBackend,
		(),
		sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, Executor>>,
		sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, Executor>>,
		(Option<Telemetry>, Option<TelemetryWorkerHandle>),
	>,
	sc_service::Error,
>
where
	RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
	RuntimeApi::RuntimeApi: sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId>,
	Executor: NativeExecutionDispatch + 'static,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = NativeElseWasmExecutor::<Executor>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let import_queue = parachain_build_import_queue::<RuntimeApi, Executor>(
		client.clone(),
		config,
		telemetry.as_ref().map(|telemetry| telemetry.handle()),
		&task_manager,
	)?;

	Ok(PartialComponents {
		client,
		backend,
		import_queue,
		task_manager,
		keystore_container,
		transaction_pool,
		select_chain: (),
		other: (telemetry, telemetry_worker_handle),
	})
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
async fn start_node_impl<RuntimeApi, Executor, BIC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	para_id: ParaId,
	build_consensus: BIC,
) -> sc_service::error::Result<NewFull<Arc<FullClient<RuntimeApi, Executor>>>>
where
	RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, Executor>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
	RuntimeApi::RuntimeApi: CollectCollationInfo<Block>,
	RuntimeApi::RuntimeApi: sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId>,
	Executor: NativeExecutionDispatch + 'static, 
	BIC: FnOnce(
		Arc<FullClient<RuntimeApi, Executor>>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, Executor>>>,
		Arc<NetworkService<Block, Hash>>,
		SyncCryptoStorePtr,
		bool,
	) -> Result<Box<dyn ParachainConsensus<Block>>, sc_service::Error>,
{
	if matches!(parachain_config.role, Role::Light) {
		return Err("Light client not supported!".into());
	}

	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial_impl(&parachain_config)?;
	let (mut telemetry, telemetry_worker_handle) = params.other;
	let client = params.client.clone();
	let backend = params.backend.clone();
	let mut task_manager = params.task_manager;

	let (relay_chain_interface, collator_key) =
		build_relay_chain_interface(polkadot_config, telemetry_worker_handle, &mut task_manager).map_err(
			|e| match e {
				polkadot_service::Error::Sub(x) => x,
				s => format!("{}", s).into(),
			},
		)?;

	let block_announce_validator = BlockAnnounceValidator::new(relay_chain_interface.clone(), para_id);

	let force_authoring = parachain_config.force_authoring;
	let validator = parachain_config.role.is_authority();
	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();
	let import_queue = cumulus_client_service::SharedImportQueue::new(params.import_queue);
	let (network, system_rpc_tx, start_network) = sc_service::build_network(sc_service::BuildNetworkParams {
		config: &parachain_config,
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		spawn_handle: task_manager.spawn_handle(),
		import_queue: import_queue.clone(),
		block_announce_validator_builder: Some(Box::new(|_| Box::new(block_announce_validator))),
		warp_sync: None,
	})?;

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();

		Box::new(move |deny_unsafe, _| -> crate::rpc::RpcExtension {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				deny_unsafe,
			};

			crate::rpc::create_full(deps)
		})
	};

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_extensions_builder,
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		config: parachain_config,
		keystore: params.keystore_container.sync_keystore(),
		backend: backend.clone(),
		network: network.clone(),
		system_rpc_tx,
		telemetry: telemetry.as_mut(),
	})?;

	let announce_block = {
		let network = network.clone();
		Arc::new(move |hash, data| network.announce_block(hash, data))
	};

	let relay_chain_slot_duration = Duration::from_secs(6);

	if validator {
		let parachain_consensus = build_consensus(
			client.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			network,
			params.keystore_container.sync_keystore(),
			force_authoring,
		)?;

		let spawner = task_manager.spawn_handle();

		let params = StartCollatorParams {
			para_id,
			block_status: client.clone(),
			announce_block,
			client: client.clone(),
			task_manager: &mut task_manager,
			relay_chain_interface,
			spawner,
			parachain_consensus,
			import_queue,
			collator_key,
			relay_chain_slot_duration,
		};

		start_collator(params).await?;
	} else {
		let params = StartFullNodeParams {
			client: client.clone(),
			announce_block,
			task_manager: &mut task_manager,
			para_id,
			relay_chain_interface,
			import_queue,
			relay_chain_slot_duration,
		};

		start_full_node(params)?;
	}

	start_network.start_network();

	Ok(NewFull { client, task_manager })
}

pub struct NewFull<C> {
	pub client: C,
	pub task_manager: TaskManager,
}

impl<C> NewFull<C> {
	/// Convert the client type using the given `func`.
	pub fn with_client<NC>(self, func: impl FnOnce(C) -> NC) -> NewFull<NC> {
		NewFull {
			client: func(self.client),
			task_manager: self.task_manager,
		}
	}
}

/// Start a normal parachain node.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
pub async fn start_node(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	para_id: ParaId,
) -> sc_service::error::Result<NewFull<Client>> {
	if parachain_config.chain_spec.is_testing_runtime() {
		start_node_impl::<testing_basilisk_runtime::RuntimeApi, TestingBasiliskExecutorDispatch, _>(
			parachain_config,
			polkadot_config,
			para_id,
			|client,
			 prometheus_registry,
			 telemetry,
			 task_manager,
			 relay_chain_interface,
			 transaction_pool,
			 sync_oracle,
			 keystore,
			 force_authoring| {
				let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

				let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
					task_manager.spawn_handle(),
					client.clone(),
					transaction_pool,
					prometheus_registry.clone(),
					telemetry.clone(),
				);

				Ok(AuraConsensus::build::<
					sp_consensus_aura::sr25519::AuthorityPair,
					_,
					_,
					_,
					_,
					_,
					_,
				>(BuildAuraConsensusParams {
					proposer_factory,
					create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
						let relay_chain_interface = relay_chain_interface.clone();
						async move {
							let parachain_inherent =
								cumulus_primitives_parachain_inherent::ParachainInherentData::create_at(
									relay_parent,
									&relay_chain_interface,
									&validation_data,
									para_id,
								)
								.await;
	
							let time = sp_timestamp::InherentDataProvider::from_system_time();
	
							let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_duration(
								*time,
								slot_duration.slot_duration(),
							);
	
							let parachain_inherent = parachain_inherent.ok_or_else(|| {
								Box::<dyn std::error::Error + Send + Sync>::from("Failed to create parachain inherent")
							})?;
							Ok((time, slot, parachain_inherent))
						}
					},
					block_import: client.clone(),
					para_client: client,
					backoff_authoring_blocks: Option::<()>::None,
					sync_oracle,
					keystore,
					force_authoring,
					slot_duration,
					// We got around 500ms for proposing
					block_proposal_slot_portion: SlotProportion::new(1f32 / 24f32),
					// And a maximum of 750ms if slots are skipped
					max_block_proposal_slot_portion: Some(SlotProportion::new(1f32 / 16f32)),
					telemetry,
				}))
			},
		)
		.await
		.map(|full| full.with_client(Client::TestingBasilisk))
	} else {
		start_node_impl::<basilisk_runtime::RuntimeApi, BasiliskExecutorDispatch, _>(
			parachain_config,
			polkadot_config,
			para_id,
			|client,
			 prometheus_registry,
			 telemetry,
			 task_manager,
			 relay_chain_interface,
			 transaction_pool,
			 sync_oracle,
			 keystore,
			 force_authoring| {
				let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

				let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
					task_manager.spawn_handle(),
					client.clone(),
					transaction_pool,
					prometheus_registry.clone(),
					telemetry.clone(),
				);

				Ok(AuraConsensus::build::<
					sp_consensus_aura::sr25519::AuthorityPair,
					_,
					_,
					_,
					_,
					_,
					_,
				>(BuildAuraConsensusParams {
					proposer_factory,
					create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
						let relay_chain_interface = relay_chain_interface.clone();
						async move {
							let parachain_inherent =
								cumulus_primitives_parachain_inherent::ParachainInherentData::create_at(
									relay_parent,
									&relay_chain_interface,
									&validation_data,
									para_id,
								)
								.await;
	
							let time = sp_timestamp::InherentDataProvider::from_system_time();
	
							let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_duration(
								*time,
								slot_duration.slot_duration(),
							);
	
							let parachain_inherent = parachain_inherent.ok_or_else(|| {
								Box::<dyn std::error::Error + Send + Sync>::from("Failed to create parachain inherent")
							})?;
							Ok((time, slot, parachain_inherent))
						}
					},
					block_import: client.clone(),
					para_client: client,
					backoff_authoring_blocks: Option::<()>::None,
					sync_oracle,
					keystore,
					force_authoring,
					slot_duration,
					// We got around 500ms for proposing
					block_proposal_slot_portion: SlotProportion::new(1f32 / 24f32),
					// And a maximum of 750ms if slots are skipped
					max_block_proposal_slot_portion: Some(SlotProportion::new(1f32 / 16f32)),
					telemetry,
				}))
			},
		)
		.await
		.map(|full| full.with_client(Client::Basilisk))
	}
}
