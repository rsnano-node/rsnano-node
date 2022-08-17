#include <nano/crypto_lib/random_pool.hpp>
#include <nano/lib/config.hpp>
#include <nano/lib/numbers.hpp>
#include <nano/lib/rsnanoutils.hpp>
#include <nano/lib/timer.hpp>
#include <nano/secure/common.hpp>
#include <nano/secure/store.hpp>

#include <crypto/cryptopp/words.h>

#include <boost/endian/conversion.hpp>
#include <boost/property_tree/json_parser.hpp>
#include <boost/variant/get.hpp>

#include <limits>
#include <queue>

#include <crypto/ed25519-donna/ed25519.h>

namespace
{
char const * dev_private_key_data = "34F0A37AAD20F4A260F0A5B3CB3D7FB50673212263E58A380BC10474BB039CE4";
}

nano::keypair nano::dev::genesis_key{ dev_private_key_data };
nano::network_params nano::dev::network_params{ nano::networks::nano_dev_network };
nano::ledger_constants & nano::dev::constants{ nano::dev::network_params.ledger };
std::shared_ptr<nano::block> & nano::dev::genesis = nano::dev::constants.genesis;

nano::network_params::network_params (nano::networks network_a) :
	work (nano::work_thresholds (0, 0, 0)),
	network (nano::network_constants (nano::work_thresholds (0, 0, 0), network_a)),
	ledger (nano::ledger_constants (nano::work_thresholds (0, 0, 0), network_a))
{
	rsnano::NetworkParamsDto dto;
	if (rsnano::rsn_network_params_create (&dto, static_cast<uint16_t> (network_a)) < 0)
		throw std::runtime_error ("could not create network params");

	work = nano::work_thresholds (dto.work);
	network = nano::network_constants (dto.network);
	ledger = std::move (nano::ledger_constants (dto.ledger));
	voting = nano::voting_constants (dto.voting);
	node = nano::node_constants (dto.node);
	portmapping = nano::portmapping_constants (dto.portmapping);
	bootstrap = nano::bootstrap_constants (dto.bootstrap);
	kdf_work = dto.kdf_work;
}

nano::network_params::network_params (rsnano::NetworkParamsDto const & dto) :
	kdf_work{ dto.kdf_work },
	work{ dto.work },
	network{ dto.network },
	ledger{ dto.ledger },
	voting{ dto.voting },
	node{ dto.node },
	portmapping{ dto.portmapping },
	bootstrap{ dto.bootstrap }
{
}

rsnano::NetworkParamsDto nano::network_params::to_dto () const
{
	rsnano::NetworkParamsDto dto;
	dto.kdf_work = kdf_work;
	dto.work = work.dto;
	dto.network = network.to_dto ();
	dto.ledger = ledger.to_dto ();
	dto.voting = voting.to_dto ();
	dto.node = node.to_dto ();
	dto.portmapping = portmapping.to_dto ();
	dto.bootstrap = bootstrap.to_dto ();
	return dto;
}

nano::ledger_constants::ledger_constants (nano::work_thresholds work_a, nano::networks network_a) :
	work (nano::work_thresholds (0, 0, 0))
{
	rsnano::LedgerConstantsDto dto;
	if (rsnano::rsn_ledger_constants_create (&dto, &work_a.dto, static_cast<uint16_t> (network_a)) < 0)
		throw std::runtime_error ("could not create ledger_constants");
	read_dto (dto);
}

nano::ledger_constants::ledger_constants (rsnano::LedgerConstantsDto const & dto) :
	work (nano::work_thresholds (0, 0, 0))
{
	read_dto (dto);
}

rsnano::LedgerConstantsDto nano::ledger_constants::to_dto () const
{
	rsnano::LedgerConstantsDto dto;
	dto.work = work.dto;
	std::copy (std::begin (zero_key.prv.bytes), std::end (zero_key.prv.bytes), std::begin (dto.priv_key));
	std::copy (std::begin (zero_key.pub.bytes), std::end (zero_key.pub.bytes), std::begin (dto.pub_key));
	std::copy (std::begin (nano_beta_account.bytes), std::end (nano_beta_account.bytes), std::begin (dto.nano_beta_account));
	std::copy (std::begin (nano_live_account.bytes), std::end (nano_live_account.bytes), std::begin (dto.nano_live_account));
	std::copy (std::begin (nano_test_account.bytes), std::end (nano_test_account.bytes), std::begin (dto.nano_test_account));

	dto.nano_dev_genesis = nano_dev_genesis->clone_handle ();
	dto.nano_beta_genesis = nano_beta_genesis->clone_handle ();
	dto.nano_live_genesis = nano_live_genesis->clone_handle ();
	dto.nano_test_genesis = nano_test_genesis->clone_handle ();
	dto.genesis = genesis->clone_handle ();
	boost::multiprecision::export_bits (genesis_amount, std::begin (dto.genesis_amount), 8);
	std::copy (std::begin (burn_account.bytes), std::end (burn_account.bytes), std::begin (dto.burn_account));
	std::copy (std::begin (nano_dev_final_votes_canary_account.bytes), std::end (nano_dev_final_votes_canary_account.bytes), std::begin (dto.nano_dev_final_votes_canary_account));
	std::copy (std::begin (nano_beta_final_votes_canary_account.bytes), std::end (nano_beta_final_votes_canary_account.bytes), std::begin (dto.nano_beta_final_votes_canary_account));
	std::copy (std::begin (nano_live_final_votes_canary_account.bytes), std::end (nano_live_final_votes_canary_account.bytes), std::begin (dto.nano_live_final_votes_canary_account));
	std::copy (std::begin (nano_test_final_votes_canary_account.bytes), std::end (nano_test_final_votes_canary_account.bytes), std::begin (dto.nano_test_final_votes_canary_account));
	std::copy (std::begin (final_votes_canary_account.bytes), std::end (final_votes_canary_account.bytes), std::begin (dto.final_votes_canary_account));
	dto.nano_dev_final_votes_canary_height = nano_dev_final_votes_canary_height;
	dto.nano_beta_final_votes_canary_height = nano_beta_final_votes_canary_height;
	dto.nano_live_final_votes_canary_height = nano_live_final_votes_canary_height;
	dto.nano_test_final_votes_canary_height = nano_test_final_votes_canary_height;
	dto.final_votes_canary_height = final_votes_canary_height;

	auto epoch_1_link{ epochs.link (nano::epoch::epoch_1) };
	auto epoch_1_signer{ epochs.signer (nano::epoch::epoch_1) };
	auto epoch_2_link{ epochs.link (nano::epoch::epoch_2) };
	auto epoch_2_signer{ epochs.signer (nano::epoch::epoch_2) };

	std::copy (std::begin (epoch_1_signer.bytes), std::end (epoch_1_signer.bytes), std::begin (dto.epoch_1_signer));
	std::copy (std::begin (epoch_1_link.bytes), std::end (epoch_1_link.bytes), std::begin (dto.epoch_1_link));
	std::copy (std::begin (epoch_2_signer.bytes), std::end (epoch_2_signer.bytes), std::begin (dto.epoch_2_signer));
	std::copy (std::begin (epoch_2_link.bytes), std::end (epoch_2_link.bytes), std::begin (dto.epoch_2_link));
	return dto;
}

void nano::ledger_constants::read_dto (rsnano::LedgerConstantsDto const & dto)
{
	work = nano::work_thresholds (dto.work);
	nano::public_key pub_key;
	nano::raw_key priv_key;
	std::copy (std::begin (dto.pub_key), std::end (dto.pub_key), std::begin (pub_key.bytes));
	std::copy (std::begin (dto.priv_key), std::end (dto.priv_key), std::begin (priv_key.bytes));
	zero_key = nano::keypair (priv_key, pub_key);
	std::copy (std::begin (dto.nano_beta_account), std::end (dto.nano_beta_account), std::begin (nano_beta_account.bytes));
	std::copy (std::begin (dto.nano_live_account), std::end (dto.nano_live_account), std::begin (nano_live_account.bytes));
	std::copy (std::begin (dto.nano_test_account), std::end (dto.nano_test_account), std::begin (nano_test_account.bytes));
	nano_dev_genesis = nano::block_handle_to_block (dto.nano_dev_genesis);
	nano_beta_genesis = nano::block_handle_to_block (dto.nano_beta_genesis);
	nano_live_genesis = nano::block_handle_to_block (dto.nano_live_genesis);
	nano_test_genesis = nano::block_handle_to_block (dto.nano_test_genesis);
	genesis = nano::block_handle_to_block (dto.genesis);
	boost::multiprecision::import_bits (genesis_amount, std::begin (dto.genesis_amount), std::end (dto.genesis_amount));
	std::copy (std::begin (dto.burn_account), std::end (dto.burn_account), std::begin (burn_account.bytes));
	std::copy (std::begin (dto.nano_dev_final_votes_canary_account), std::end (dto.nano_dev_final_votes_canary_account), std::begin (nano_dev_final_votes_canary_account.bytes));
	std::copy (std::begin (dto.nano_beta_final_votes_canary_account), std::end (dto.nano_beta_final_votes_canary_account), std::begin (nano_beta_final_votes_canary_account.bytes));
	std::copy (std::begin (dto.nano_live_final_votes_canary_account), std::end (dto.nano_live_final_votes_canary_account), std::begin (nano_live_final_votes_canary_account.bytes));
	std::copy (std::begin (dto.nano_test_final_votes_canary_account), std::end (dto.nano_test_final_votes_canary_account), std::begin (nano_test_final_votes_canary_account.bytes));
	std::copy (std::begin (dto.final_votes_canary_account), std::end (dto.final_votes_canary_account), std::begin (final_votes_canary_account.bytes));
	nano_dev_final_votes_canary_height = dto.nano_dev_final_votes_canary_height;
	nano_beta_final_votes_canary_height = dto.nano_beta_final_votes_canary_height;
	nano_live_final_votes_canary_height = dto.nano_live_final_votes_canary_height;
	nano_test_final_votes_canary_height = dto.nano_test_final_votes_canary_height;
	final_votes_canary_height = dto.final_votes_canary_height;

	nano::account epoch_v1_signer;
	std::copy (std::begin (dto.epoch_1_signer), std::end (dto.epoch_1_signer), std::begin (epoch_v1_signer.bytes));
	nano::link epoch_v1_link;
	std::copy (std::begin (dto.epoch_1_link), std::end (dto.epoch_1_link), std::begin (epoch_v1_link.bytes));
	nano::account epoch_v2_signer;
	std::copy (std::begin (dto.epoch_2_signer), std::end (dto.epoch_2_signer), std::begin (epoch_v2_signer.bytes));
	nano::link epoch_v2_link;
	std::copy (std::begin (dto.epoch_2_link), std::end (dto.epoch_2_link), std::begin (epoch_v2_link.bytes));

	epochs.add (nano::epoch::epoch_1, epoch_v1_signer, epoch_v1_link);
	epochs.add (nano::epoch::epoch_2, epoch_v2_signer, epoch_v2_link);
}

nano::hardened_constants & nano::hardened_constants::get ()
{
	static hardened_constants instance{};
	return instance;
}

nano::hardened_constants::hardened_constants () :
	not_an_account{},
	random_128{}
{
	rsnano::rsn_hardened_constants_get (not_an_account.bytes.data (), random_128.bytes.data ());
}

nano::node_constants::node_constants (nano::network_constants & network_constants)
{
	rsnano::NodeConstantsDto dto;
	auto network_dto{ network_constants.to_dto () };
	if (rsnano::rsn_node_constants_create (&network_dto, &dto) < 0)
		throw std::runtime_error ("could not create node constants");
	read_dto (dto);
}

nano::node_constants::node_constants (rsnano::NodeConstantsDto const & dto)
{
	read_dto (dto);
}

void nano::node_constants::read_dto (rsnano::NodeConstantsDto const & dto)
{
	backup_interval = std::chrono::minutes (dto.backup_interval_m);
	search_pending_interval = std::chrono::seconds (dto.search_pending_interval_s);
	unchecked_cleaning_interval = std::chrono::minutes (dto.unchecked_cleaning_interval_m);
	process_confirmed_interval = std::chrono::milliseconds (dto.process_confirmed_interval_ms);
	max_weight_samples = dto.max_weight_samples;
	weight_period = dto.weight_period;
}

rsnano::NodeConstantsDto nano::node_constants::to_dto () const
{
	rsnano::NodeConstantsDto dto;
	dto.backup_interval_m = backup_interval.count ();
	dto.search_pending_interval_s = search_pending_interval.count ();
	dto.unchecked_cleaning_interval_m = unchecked_cleaning_interval.count ();
	dto.process_confirmed_interval_ms = process_confirmed_interval.count ();
	dto.max_weight_samples = max_weight_samples;
	dto.weight_period = weight_period;
	return dto;
}

nano::voting_constants::voting_constants (nano::network_constants & network_constants)
{
	auto network_dto{ network_constants.to_dto () };
	rsnano::VotingConstantsDto dto;
	if (rsnano::rsn_voting_constants_create (&network_dto, &dto) < 0)
		throw std::runtime_error ("could not create voting constants");
	max_cache = dto.max_cache;
	delay = std::chrono::seconds (dto.delay_s);
}

nano::voting_constants::voting_constants (rsnano::VotingConstantsDto const & dto)
{
	max_cache = dto.max_cache;
	delay = std::chrono::seconds (dto.delay_s);
}

rsnano::VotingConstantsDto nano::voting_constants::to_dto () const
{
	rsnano::VotingConstantsDto result;
	result.max_cache = max_cache;
	result.delay_s = delay.count ();
	return result;
}

nano::portmapping_constants::portmapping_constants (nano::network_constants & network_constants)
{
	rsnano::PortmappingConstantsDto dto;
	auto network_dto{ network_constants.to_dto () };
	if (rsnano::rsn_portmapping_constants_create (&network_dto, &dto) < 0)
		throw std::runtime_error ("could not create portmapping constants");
	lease_duration = std::chrono::seconds (dto.lease_duration_s);
	health_check_period = std::chrono::seconds (dto.health_check_period_s);
}

nano::portmapping_constants::portmapping_constants (rsnano::PortmappingConstantsDto const & dto)
{
	lease_duration = std::chrono::seconds (dto.lease_duration_s);
	health_check_period = std::chrono::seconds (dto.health_check_period_s);
}

rsnano::PortmappingConstantsDto nano::portmapping_constants::to_dto () const
{
	rsnano::PortmappingConstantsDto dto;
	dto.lease_duration_s = lease_duration.count ();
	dto.health_check_period_s = health_check_period.count ();
	return dto;
}

nano::bootstrap_constants::bootstrap_constants (nano::network_constants & network_constants)
{
	auto network_dto{ network_constants.to_dto () };
	rsnano::BootstrapConstantsDto dto;
	if (rsnano::rsn_bootstrap_constants_create (&network_dto, &dto) < 0)
		throw std::runtime_error ("could not create bootstrap constants");
	read_dto (dto);
}

nano::bootstrap_constants::bootstrap_constants (rsnano::BootstrapConstantsDto const & dto)
{
	read_dto (dto);
}

rsnano::BootstrapConstantsDto nano::bootstrap_constants::to_dto () const
{
	rsnano::BootstrapConstantsDto dto;
	dto.lazy_max_pull_blocks = lazy_max_pull_blocks;
	dto.lazy_min_pull_blocks = lazy_min_pull_blocks;
	dto.frontier_retry_limit = frontier_retry_limit;
	dto.lazy_retry_limit = lazy_retry_limit;
	dto.lazy_destinations_retry_limit = lazy_destinations_retry_limit;
	dto.gap_cache_bootstrap_start_interval_ms = gap_cache_bootstrap_start_interval.count ();
	dto.default_frontiers_age_seconds = default_frontiers_age_seconds;
	return dto;
}

void nano::bootstrap_constants::read_dto (rsnano::BootstrapConstantsDto const & dto)
{
	lazy_max_pull_blocks = dto.lazy_max_pull_blocks;
	lazy_min_pull_blocks = dto.lazy_min_pull_blocks;
	frontier_retry_limit = dto.frontier_retry_limit;
	lazy_retry_limit = dto.lazy_retry_limit;
	lazy_destinations_retry_limit = dto.lazy_destinations_retry_limit;
	gap_cache_bootstrap_start_interval = std::chrono::milliseconds (dto.gap_cache_bootstrap_start_interval_ms);
	default_frontiers_age_seconds = dto.default_frontiers_age_seconds;
}

// Create a new random keypair
nano::keypair::keypair ()
{
	random_pool::generate_block (prv.bytes.data (), prv.bytes.size ());
	ed25519_publickey (prv.bytes.data (), pub.bytes.data ());
}

// Create a keypair given a private key
nano::keypair::keypair (nano::raw_key && prv_a) :
	prv (std::move (prv_a))
{
	ed25519_publickey (prv.bytes.data (), pub.bytes.data ());
}

// Create a keypair given a hex string of the private key
nano::keypair::keypair (std::string const & prv_a)
{
	[[maybe_unused]] auto error (prv.decode_hex (prv_a));
	debug_assert (!error);
	ed25519_publickey (prv.bytes.data (), pub.bytes.data ());
}

nano::keypair::keypair (nano::raw_key const & priv_key_a, nano::public_key const & pub_key_a) :
	prv (priv_key_a),
	pub (pub_key_a)
{
}

nano::keypair::keypair (const nano::keypair & other_a) :
	prv{ other_a.prv },
	pub{ other_a.pub }
{
}

// Serialize a block prefixed with an 8-bit typecode
void nano::serialize_block (nano::stream & stream_a, nano::block const & block_a)
{
	write (stream_a, block_a.type ());
	block_a.serialize (stream_a);
}

nano::account_info::account_info (nano::block_hash const & head_a, nano::account const & representative_a, nano::block_hash const & open_block_a, nano::amount const & balance_a, uint64_t modified_a, uint64_t block_count_a, nano::epoch epoch_a) :
	head (head_a),
	representative (representative_a),
	open_block (open_block_a),
	balance (balance_a),
	modified (modified_a),
	block_count (block_count_a),
	epoch_m (epoch_a)
{
}

bool nano::account_info::deserialize (nano::stream & stream_a)
{
	auto error (false);
	try
	{
		nano::read (stream_a, head.bytes);
		nano::read (stream_a, representative.bytes);
		nano::read (stream_a, open_block.bytes);
		nano::read (stream_a, balance.bytes);
		nano::read (stream_a, modified);
		nano::read (stream_a, block_count);
		nano::read (stream_a, epoch_m);
	}
	catch (std::runtime_error const &)
	{
		error = true;
	}

	return error;
}

bool nano::account_info::operator== (nano::account_info const & other_a) const
{
	return head == other_a.head && representative == other_a.representative && open_block == other_a.open_block && balance == other_a.balance && modified == other_a.modified && block_count == other_a.block_count && epoch () == other_a.epoch ();
}

bool nano::account_info::operator!= (nano::account_info const & other_a) const
{
	return !(*this == other_a);
}

size_t nano::account_info::db_size () const
{
	debug_assert (reinterpret_cast<uint8_t const *> (this) == reinterpret_cast<uint8_t const *> (&head));
	debug_assert (reinterpret_cast<uint8_t const *> (&head) + sizeof (head) == reinterpret_cast<uint8_t const *> (&representative));
	debug_assert (reinterpret_cast<uint8_t const *> (&representative) + sizeof (representative) == reinterpret_cast<uint8_t const *> (&open_block));
	debug_assert (reinterpret_cast<uint8_t const *> (&open_block) + sizeof (open_block) == reinterpret_cast<uint8_t const *> (&balance));
	debug_assert (reinterpret_cast<uint8_t const *> (&balance) + sizeof (balance) == reinterpret_cast<uint8_t const *> (&modified));
	debug_assert (reinterpret_cast<uint8_t const *> (&modified) + sizeof (modified) == reinterpret_cast<uint8_t const *> (&block_count));
	debug_assert (reinterpret_cast<uint8_t const *> (&block_count) + sizeof (block_count) == reinterpret_cast<uint8_t const *> (&epoch_m));
	return sizeof (head) + sizeof (representative) + sizeof (open_block) + sizeof (balance) + sizeof (modified) + sizeof (block_count) + sizeof (epoch_m);
}

nano::epoch nano::account_info::epoch () const
{
	return epoch_m;
}

nano::pending_info::pending_info (nano::account const & source_a, nano::amount const & amount_a, nano::epoch epoch_a) :
	source (source_a),
	amount (amount_a),
	epoch (epoch_a)
{
}

bool nano::pending_info::deserialize (nano::stream & stream_a)
{
	auto error (false);
	try
	{
		nano::read (stream_a, source.bytes);
		nano::read (stream_a, amount.bytes);
		nano::read (stream_a, epoch);
	}
	catch (std::runtime_error const &)
	{
		error = true;
	}

	return error;
}

size_t nano::pending_info::db_size () const
{
	return sizeof (source) + sizeof (amount) + sizeof (epoch);
}

bool nano::pending_info::operator== (nano::pending_info const & other_a) const
{
	return source == other_a.source && amount == other_a.amount && epoch == other_a.epoch;
}

nano::pending_key::pending_key (nano::account const & account_a, nano::block_hash const & hash_a) :
	account (account_a),
	hash (hash_a)
{
}

bool nano::pending_key::deserialize (nano::stream & stream_a)
{
	auto error (false);
	try
	{
		nano::read (stream_a, account.bytes);
		nano::read (stream_a, hash.bytes);
	}
	catch (std::runtime_error const &)
	{
		error = true;
	}

	return error;
}

bool nano::pending_key::operator== (nano::pending_key const & other_a) const
{
	return account == other_a.account && hash == other_a.hash;
}

nano::account const & nano::pending_key::key () const
{
	return account;
}

nano::unchecked_info::unchecked_info () :
	handle (rsnano::rsn_unchecked_info_create ())
{
}

nano::unchecked_info::unchecked_info (nano::unchecked_info const & other_a) :
	handle (rsnano::rsn_unchecked_info_clone (other_a.handle))
{
}

nano::unchecked_info::unchecked_info (nano::unchecked_info && other_a) :
	handle (other_a.handle)
{
	other_a.handle = nullptr;
}

nano::unchecked_info::unchecked_info (rsnano::UncheckedInfoHandle * handle_a) :
	handle (handle_a)
{
}

nano::unchecked_info::unchecked_info (std::shared_ptr<nano::block> const & block_a, nano::account const & account_a, nano::signature_verification verified_a) :
	handle (rsnano::rsn_unchecked_info_create2 (block_a->get_handle (), account_a.bytes.data (), static_cast<uint8_t> (verified_a)))
{
}

nano::unchecked_info::unchecked_info (std::shared_ptr<nano::block> const & block) :
	unchecked_info{ block, block->account (), nano::signature_verification::unknown }
{
}

nano::unchecked_info::~unchecked_info ()
{
	if (handle != nullptr)
		rsnano::rsn_unchecked_info_destroy (handle);
}

nano::unchecked_info & nano::unchecked_info::operator= (const nano::unchecked_info & other_a)
{
	if (handle != nullptr)
		rsnano::rsn_unchecked_info_destroy (handle);

	handle = rsnano::rsn_unchecked_info_clone (other_a.handle);
	return *this;
}

std::shared_ptr<nano::block> nano::unchecked_info::get_block () const
{
	auto block_handle = rsnano::rsn_unchecked_info_block (handle);
	return block_handle_to_block (block_handle);
}

nano::account nano::unchecked_info::get_account () const
{
	nano::account account;
	rsnano::rsn_unchecked_info_account (handle, account.bytes.data ());
	return account;
}

nano::signature_verification nano::unchecked_info::get_verified () const
{
	return static_cast<nano::signature_verification> (rsnano::rsn_unchecked_info_verified (handle));
}

void nano::unchecked_info::set_verified (nano::signature_verification verified)
{
	rsnano::rsn_unchecked_info_verified_set (handle, static_cast<uint8_t> (verified));
}

void nano::unchecked_info::serialize (nano::stream & stream_a) const
{
	auto modified = rsnano::rsn_unchecked_info_modified (handle);
	auto block = get_block ();
	auto acc = get_account ();
	nano::serialize_block (stream_a, *block);
	nano::write (stream_a, acc.bytes);
	nano::write (stream_a, modified);
	nano::write (stream_a, get_verified ());
}

bool nano::unchecked_info::deserialize (nano::stream & stream_a)
{
	auto block = nano::deserialize_block (stream_a);
	bool error (block == nullptr);
	if (!error)
	{
		rsnano::rsn_unchecked_info_block_set (handle, block->get_handle ());
		try
		{
			nano::account acc;
			nano::read (stream_a, acc.bytes);
			rsnano::rsn_unchecked_info_account_set (handle, acc.bytes.data ());
			uint64_t modified;
			nano::read (stream_a, modified);
			rsnano::rsn_unchecked_info_modified_set (handle, modified);
			nano::signature_verification v;
			nano::read (stream_a, v);
			rsnano::rsn_unchecked_info_verified_set (handle, static_cast<uint8_t> (v));
		}
		catch (std::runtime_error const &)
		{
			error = true;
		}
	}
	return error;
}

uint64_t nano::unchecked_info::modified () const
{
	return rsnano::rsn_unchecked_info_modified (handle);
}

nano::endpoint_key::endpoint_key (std::array<uint8_t, 16> const & address_a, uint16_t port_a) :
	address (address_a), network_port (boost::endian::native_to_big (port_a))
{
}

std::array<uint8_t, 16> const & nano::endpoint_key::address_bytes () const
{
	return address;
}

uint16_t nano::endpoint_key::port () const
{
	return boost::endian::big_to_native (network_port);
}

nano::confirmation_height_info::confirmation_height_info (uint64_t confirmation_height_a, nano::block_hash const & confirmed_frontier_a) :
	height (confirmation_height_a),
	frontier (confirmed_frontier_a)
{
}

void nano::confirmation_height_info::serialize (nano::stream & stream_a) const
{
	nano::write (stream_a, height);
	nano::write (stream_a, frontier);
}

bool nano::confirmation_height_info::deserialize (nano::stream & stream_a)
{
	auto error (false);
	try
	{
		nano::read (stream_a, height);
		nano::read (stream_a, frontier);
	}
	catch (std::runtime_error const &)
	{
		error = true;
	}
	return error;
}

nano::block_info::block_info (nano::account const & account_a, nano::amount const & balance_a) :
	account (account_a),
	balance (balance_a)
{
}

bool nano::vote::operator== (nano::vote const & other_a) const
{
	return rsnano::rsn_vote_equals (handle, other_a.handle);
}

bool nano::vote::operator!= (nano::vote const & other_a) const
{
	return !(*this == other_a);
}

std::vector<nano::block_hash> read_block_hashes (rsnano::VoteHandle const * handle)
{
	auto hashes_dto{ rsnano::rsn_vote_hashes (handle) };
	std::vector<nano::block_hash> hashes;
	hashes.resize (hashes_dto.count);
	for (auto i (0); i < hashes_dto.count; ++i)
	{
		std::copy (std::begin (hashes_dto.hashes[i]), std::end (hashes_dto.hashes[i]), std::begin (hashes[i].bytes));
	}
	rsnano::rsn_vote_hashes_destroy (hashes_dto.handle);
	return hashes;
}

void nano::vote::serialize_json (boost::property_tree::ptree & tree) const
{
	rsnano::rsn_vote_serialize_json (handle, &tree);
}

std::string nano::vote::to_json () const
{
	std::stringstream stream;
	boost::property_tree::ptree tree;
	serialize_json (tree);
	boost::property_tree::write_json (stream, tree);
	return stream.str ();
}

/**
 * Returns the timestamp of the vote (with the duration bits masked, set to zero)
 * If it is a final vote, all the bits including duration bits are returned as they are, all FF
 */
uint64_t nano::vote::timestamp () const
{
	return rsnano::rsn_vote_timestamp (handle);
}

uint8_t nano::vote::duration_bits () const
{
	return rsnano::rsn_vote_duration_bits (handle);
}

std::chrono::milliseconds nano::vote::duration () const
{
	return std::chrono::milliseconds{ rsnano::rsn_vote_duration_ms (handle) };
}

std::vector<nano::block_hash> nano::vote::hashes () const
{
	auto hashes{ read_block_hashes (handle) };
	return hashes;
}

nano::vote::vote () :
	handle (rsnano::rsn_vote_create ())
{
}

nano::vote::vote (rsnano::VoteHandle * handle_a) :
	handle (handle_a)
{
}

nano::vote::vote (nano::vote const & other_a) :
	handle (rsnano::rsn_vote_copy (other_a.handle))
{
}

nano::vote::vote (nano::vote && other_a) :
	handle (other_a.handle)
{
	other_a.handle = nullptr;
}

nano::vote::vote (nano::account const & account) :
	handle (rsnano::rsn_vote_create ())
{
	rsnano::rsn_vote_account_set (handle, account.bytes.data ());
}

nano::vote::vote (bool & error_a, nano::stream & stream_a) :
	handle{ rsnano::rsn_vote_create () }
{
	error_a = deserialize (stream_a);
}

nano::vote::vote (nano::account const & account_a, nano::raw_key const & prv_a, uint64_t timestamp_a, uint8_t duration, std::vector<nano::block_hash> const & hashes)
{
	handle = rsnano::rsn_vote_create2 (account_a.bytes.data (), prv_a.bytes.data (), timestamp_a, duration, reinterpret_cast<const uint8_t (*)[32]> (hashes.data ()), hashes.size ());
}

nano::vote::~vote ()
{
	if (handle != nullptr)
	{
		rsnano::rsn_vote_destroy (handle);
	}
}

std::string nano::vote::hashes_string () const
{
	auto dto{ rsnano::rsn_vote_hashes_string (handle) };
	return rsnano::convert_dto_to_string (dto);
}

std::string const nano::vote::hash_prefix = "vote ";

nano::block_hash nano::vote::hash () const
{
	nano::block_hash result;
	rsnano::rsn_vote_hash (handle, result.bytes.data ());
	return result;
}

nano::block_hash nano::vote::full_hash () const
{
	nano::block_hash result;
	rsnano::rsn_vote_full_hash (handle, result.bytes.data ());
	return result;
}

void nano::vote::serialize (nano::stream & stream_a) const
{
	auto result = rsnano::rsn_vote_serialize (handle, &stream_a);
	if (result != 0)
	{
		throw std::runtime_error ("Could not serialize vote");
	}
}

bool nano::vote::deserialize (nano::stream & stream_a)
{
	auto error = rsnano::rsn_vote_deserialize (handle, &stream_a) != 0;
	return error;
}

bool nano::vote::validate () const
{
	return rsnano::rsn_vote_validate (handle);
}

nano::account nano::vote::account () const
{
	nano::account account;
	rsnano::rsn_vote_account (handle, account.bytes.data ());
	return account;
}

nano::signature nano::vote::signature () const
{
	nano::signature signature;
	rsnano::rsn_vote_signature (handle, signature.bytes.data ());
	return signature;
}

void nano::vote::flip_signature_bit_0 ()
{
	nano::signature signature;
	rsnano::rsn_vote_signature (handle, signature.bytes.data ());
	signature.bytes[0] ^= 1;
	rsnano::rsn_vote_signature_set (handle, signature.bytes.data ());
}

rsnano::VoteHandle * nano::vote::get_handle () const
{
	return handle;
}

const void * nano::vote::get_rust_data_pointer () const
{
	return rsnano::rsn_vote_rust_data_pointer (handle);
}

nano::block_hash nano::iterate_vote_blocks_as_hash::operator() (nano::block_hash const & item) const
{
	return item;
}

nano::vote_uniquer::vote_uniquer (nano::block_uniquer & uniquer_a) :
	handle (rsnano::rsn_vote_uniquer_create ())
{
}

nano::vote_uniquer::~vote_uniquer ()
{
	if (handle != nullptr)
	{
		rsnano::rsn_vote_uniquer_destroy (handle);
	}
}

std::shared_ptr<nano::vote> nano::vote_uniquer::unique (std::shared_ptr<nano::vote> const & vote_a)
{
	if (vote_a == nullptr)
	{
		return nullptr;
	}
	auto uniqued (rsnano::rsn_vote_uniquer_unique (handle, vote_a->get_handle ()));
	if (uniqued == vote_a->get_handle ())
	{
		return vote_a;
	}
	else
	{
		return std::make_shared<nano::vote> (uniqued);
	}
}

size_t nano::vote_uniquer::size ()
{
	return rsnano::rsn_vote_uniquer_size (handle);
}

std::unique_ptr<nano::container_info_component> nano::collect_container_info (vote_uniquer & vote_uniquer, std::string const & name)
{
	auto count = vote_uniquer.size ();
	auto sizeof_element = sizeof (vote_uniquer::value_type);
	auto composite = std::make_unique<container_info_composite> (name);
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "votes", count, sizeof_element }));
	return composite;
}

nano::wallet_id nano::random_wallet_id ()
{
	nano::wallet_id wallet_id;
	nano::uint256_union dummy_secret;
	random_pool::generate_block (dummy_secret.bytes.data (), dummy_secret.bytes.size ());
	ed25519_publickey (dummy_secret.bytes.data (), wallet_id.bytes.data ());
	return wallet_id;
}

nano::unchecked_key::unchecked_key (nano::hash_or_account const & dependency) :
	unchecked_key{ dependency, 0 }
{
}

nano::unchecked_key::unchecked_key (nano::hash_or_account const & previous_a, nano::block_hash const & hash_a) :
	previous (previous_a.as_block_hash ()),
	hash (hash_a)
{
}

nano::unchecked_key::unchecked_key (nano::uint512_union const & union_a) :
	previous (union_a.uint256s[0].number ()),
	hash (union_a.uint256s[1].number ())
{
}

bool nano::unchecked_key::deserialize (nano::stream & stream_a)
{
	auto error (false);
	try
	{
		nano::read (stream_a, previous.bytes);
		nano::read (stream_a, hash.bytes);
	}
	catch (std::runtime_error const &)
	{
		error = true;
	}

	return error;
}

bool nano::unchecked_key::operator== (nano::unchecked_key const & other_a) const
{
	return previous == other_a.previous && hash == other_a.hash;
}

bool nano::unchecked_key::operator< (nano::unchecked_key const & other_a) const
{
	return previous != other_a.previous ? previous < other_a.previous : hash < other_a.hash;
}

nano::block_hash const & nano::unchecked_key::key () const
{
	return previous;
}

nano::generate_cache::generate_cache () :
	handle{ rsnano::rsn_generate_cache_create () }
{
}

nano::generate_cache::generate_cache (rsnano::GenerateCacheHandle * handle_a) :
	handle{ handle_a }
{
}

void nano::generate_cache::enable_all ()
{
	rsnano::rsn_generate_cache_enable_all (handle);
}

nano::generate_cache::generate_cache (nano::generate_cache && other_a) noexcept :
	handle{ other_a.handle }
{
	other_a.handle = nullptr;
}

nano::generate_cache::generate_cache (const nano::generate_cache & other_a) :
	handle{ rsnano::rsn_generate_cache_clone (other_a.handle) }
{
}

nano::generate_cache::~generate_cache ()
{
	if (handle)
		rsnano::rsn_generate_cache_destroy (handle);
}

nano::generate_cache & nano::generate_cache::operator= (nano::generate_cache && other_a)
{
	if (handle != nullptr)
		rsnano::rsn_generate_cache_destroy (handle);
	handle = other_a.handle;
	other_a.handle = nullptr;
	return *this;
}
nano::generate_cache & nano::generate_cache::operator= (const nano::generate_cache & other_a)
{
	if (handle != nullptr)
		rsnano::rsn_generate_cache_destroy (handle);
	handle = rsnano::rsn_generate_cache_clone (other_a.handle);
	return *this;
}
bool nano::generate_cache::reps () const
{
	return rsnano::rsn_generate_cache_reps (handle);
}
void nano::generate_cache::enable_reps (bool enable)
{
	rsnano::rsn_generate_cache_set_reps (handle, enable);
}
bool nano::generate_cache::cemented_count () const
{
	return rsnano::rsn_generate_cache_cemented_count (handle);
}
void nano::generate_cache::enable_cemented_count (bool enable)
{
	rsnano::rsn_generate_cache_set_cemented_count (handle, enable);
}
bool nano::generate_cache::unchecked_count () const
{
	return rsnano::rsn_generate_cache_unchecked_count (handle);
}
void nano::generate_cache::enable_unchecked_count (bool enable)
{
	rsnano::rsn_generate_cache_set_unchecked_count (handle, enable);
}
bool nano::generate_cache::account_count () const
{
	return rsnano::rsn_generate_cache_account_count (handle);
}
void nano::generate_cache::enable_account_count (bool enable)
{
	rsnano::rsn_generate_cache_set_account_count (handle, enable);
}
bool nano::generate_cache::block_count () const
{
	return rsnano::rsn_generate_cache_block_count (handle);
}
void nano::generate_cache::enable_block_count (bool enable)
{
	rsnano::rsn_generate_cache_set_account_count (handle, enable);
}
