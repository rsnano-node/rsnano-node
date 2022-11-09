#include <nano/crypto_lib/random_pool.hpp>
#include <nano/lib/config.hpp>
#include <nano/lib/jsonconfig.hpp>
#include <nano/lib/rpcconfig.hpp>
#include <nano/lib/rsnanoutils.hpp>
#include <nano/lib/tomlconfig.hpp>
#include <nano/node/nodeconfig.hpp>
#include <nano/node/transport/transport.hpp>

#include <boost/format.hpp>

namespace
{
char const * preconfigured_peers_key = "preconfigured_peers";
char const * signature_checker_threads_key = "signature_checker_threads";
char const * pow_sleep_interval_key = "pow_sleep_interval";
}

rsnano::NodeConfigDto to_node_config_dto (nano::node_config const & config)
{
	rsnano::NodeConfigDto dto;
	dto.peering_port = config.peering_port.value_or (0);
	dto.peering_port_defined = config.peering_port.has_value ();
	dto.bootstrap_fraction_numerator = config.bootstrap_fraction_numerator;
	std::copy (std::begin (config.receive_minimum.bytes), std::end (config.receive_minimum.bytes), std::begin (dto.receive_minimum));
	std::copy (std::begin (config.online_weight_minimum.bytes), std::end (config.online_weight_minimum.bytes), std::begin (dto.online_weight_minimum));
	dto.election_hint_weight_percent = config.election_hint_weight_percent;
	dto.password_fanout = config.password_fanout;
	dto.io_threads = config.io_threads;
	dto.network_threads = config.network_threads;
	dto.work_threads = config.work_threads;
	dto.signature_checker_threads = config.signature_checker_threads;
	dto.enable_voting = config.enable_voting;
	dto.bootstrap_connections = config.bootstrap_connections;
	dto.bootstrap_connections_max = config.bootstrap_connections_max;
	dto.bootstrap_initiator_threads = config.bootstrap_initiator_threads;
	dto.bootstrap_serving_threads = config.bootstrap_serving_threads;
	dto.bootstrap_frontier_request_count = config.bootstrap_frontier_request_count;
	dto.block_processor_batch_max_time_ms = config.block_processor_batch_max_time.count ();
	dto.allow_local_peers = config.allow_local_peers;
	std::copy (std::begin (config.vote_minimum.bytes), std::end (config.vote_minimum.bytes), std::begin (dto.vote_minimum));
	dto.vote_generator_delay_ms = config.vote_generator_delay.count ();
	dto.vote_generator_threshold = config.vote_generator_threshold;
	dto.unchecked_cutoff_time_s = config.unchecked_cutoff_time.count ();
	dto.tcp_io_timeout_s = config.tcp_io_timeout.count ();
	dto.pow_sleep_interval_ns = config.pow_sleep_interval.count ();
	std::copy (config.external_address.begin (), config.external_address.end (), std::begin (dto.external_address));
	dto.external_address_len = config.external_address.length ();
	dto.external_port = config.external_port;
	dto.tcp_incoming_connections_max = config.tcp_incoming_connections_max;
	dto.use_memory_pools = config.use_memory_pools;
	dto.confirmation_history_size = config.confirmation_history_size;
	dto.active_elections_size = config.active_elections_size;
	dto.active_elections_hinted_limit_percentage = config.active_elections_hinted_limit_percentage;
	dto.bandwidth_limit = config.bandwidth_limit;
	dto.bandwidth_limit_burst_ratio = config.bandwidth_limit_burst_ratio;
	dto.bootstrap_bandwidth_limit = config.bootstrap_bandwidth_limit;
	dto.bootstrap_bandwidth_burst_ratio = config.bootstrap_bandwidth_burst_ratio;
	dto.conf_height_processor_batch_min_time_ms = config.conf_height_processor_batch_min_time.count ();
	dto.backup_before_upgrade = config.backup_before_upgrade;
	dto.max_work_generate_multiplier = config.max_work_generate_multiplier;
	dto.frontiers_confirmation = static_cast<uint8_t> (config.frontiers_confirmation);
	dto.max_queued_requests = config.max_queued_requests;
	std::copy (std::begin (config.rep_crawler_weight_minimum.bytes), std::end (config.rep_crawler_weight_minimum.bytes), std::begin (dto.rep_crawler_weight_minimum));
	dto.work_peers_count = config.work_peers.size ();
	for (auto i = 0; i < config.work_peers.size (); i++)
	{
		std::copy (config.work_peers[i].first.begin (), config.work_peers[i].first.end (), std::begin (dto.work_peers[i].address));
		dto.work_peers[i].address_len = config.work_peers[i].first.size ();
		dto.work_peers[i].port = config.work_peers[i].second;
	}
	dto.secondary_work_peers_count = config.secondary_work_peers.size ();
	for (auto i = 0; i < config.secondary_work_peers.size (); i++)
	{
		std::copy (config.secondary_work_peers[i].first.begin (), config.secondary_work_peers[i].first.end (), std::begin (dto.secondary_work_peers[i].address));
		dto.secondary_work_peers[i].address_len = config.secondary_work_peers[i].first.size ();
		dto.secondary_work_peers[i].port = config.secondary_work_peers[i].second;
	}
	dto.preconfigured_peers_count = config.preconfigured_peers.size ();
	for (auto i = 0; i < config.preconfigured_peers.size (); i++)
	{
		std::copy (config.preconfigured_peers[i].begin (), config.preconfigured_peers[i].end (), std::begin (dto.preconfigured_peers[i].address));
		dto.preconfigured_peers[i].address_len = config.preconfigured_peers[i].size ();
	}
	for (auto i = 0; i < config.preconfigured_representatives.size (); i++)
	{
		std::copy (std::begin (config.preconfigured_representatives[i].bytes), std::end (config.preconfigured_representatives[i].bytes), std::begin (dto.preconfigured_representatives[i]));
		dto.preconfigured_representatives_count = config.preconfigured_representatives.size ();
	}
	dto.preconfigured_representatives_count = config.preconfigured_representatives.size ();
	dto.max_pruning_age_s = config.max_pruning_age.count ();
	dto.max_pruning_depth = config.max_pruning_depth;
	std::copy (config.callback_address.begin (), config.callback_address.end (), std::begin (dto.callback_address));
	dto.callback_address_len = config.callback_address.size ();
	std::copy (config.callback_target.begin (), config.callback_target.end (), std::begin (dto.callback_target));
	dto.callback_target_len = config.callback_target.size ();
	dto.callback_port = config.callback_port;
	dto.logging = config.logging.to_dto ();
	dto.websocket_config = config.websocket_config.to_dto ();
	dto.ipc_config = config.ipc_config.to_dto ();
	dto.diagnostics_config = config.diagnostics_config.to_dto ();
	dto.stat_config = config.stat_config.to_dto ();
	dto.lmdb_config = config.lmdb_config.to_dto ();
	return dto;
}

nano::node_config::node_config (nano::network_params & network_params) :
	node_config (std::nullopt, nano::logging (), network_params)
{
}

nano::node_config::node_config (const std::optional<uint16_t> & peering_port_a, nano::logging const & logging_a, nano::network_params & network_params) :
	network_params{ network_params },
	logging{ logging_a },
	websocket_config{ network_params.network },
	ipc_config (network_params.network)
{
	rsnano::NodeConfigDto dto;
	auto network_params_dto{ network_params.to_dto () };
	auto logging_dto{ logging.to_dto () };
	rsnano::rsn_node_config_create (&dto, peering_port_a.value_or (0), peering_port_a.has_value (), &logging_dto, &network_params_dto.dto);
	load_dto (dto);
}

rsnano::NodeConfigDto nano::node_config::to_dto () const
{
	return to_node_config_dto (*this);
}

void nano::node_config::load_dto (rsnano::NodeConfigDto & dto)
{
	if (dto.peering_port_defined)
	{
		peering_port = dto.peering_port;
	}
	else
	{
		peering_port = std::nullopt;
	}

	bootstrap_fraction_numerator = dto.bootstrap_fraction_numerator;
	std::copy (std::begin (dto.receive_minimum), std::end (dto.receive_minimum), std::begin (receive_minimum.bytes));
	std::copy (std::begin (dto.online_weight_minimum), std::end (dto.online_weight_minimum), std::begin (online_weight_minimum.bytes));
	election_hint_weight_percent = dto.election_hint_weight_percent;
	password_fanout = dto.password_fanout;
	io_threads = dto.io_threads;
	network_threads = dto.network_threads;
	work_threads = dto.work_threads;
	signature_checker_threads = dto.signature_checker_threads;
	enable_voting = dto.enable_voting;
	bootstrap_connections = dto.bootstrap_connections;
	bootstrap_connections_max = dto.bootstrap_connections_max;
	bootstrap_initiator_threads = dto.bootstrap_initiator_threads;
	bootstrap_serving_threads = dto.bootstrap_serving_threads;
	bootstrap_frontier_request_count = dto.bootstrap_frontier_request_count;
	block_processor_batch_max_time = std::chrono::milliseconds (dto.block_processor_batch_max_time_ms);
	allow_local_peers = dto.allow_local_peers;
	std::copy (std::begin (dto.vote_minimum), std::end (dto.vote_minimum), std::begin (vote_minimum.bytes));
	vote_generator_delay = std::chrono::milliseconds (dto.vote_generator_delay_ms);
	vote_generator_threshold = dto.vote_generator_threshold;
	unchecked_cutoff_time = std::chrono::seconds (dto.unchecked_cutoff_time_s);
	tcp_io_timeout = std::chrono::seconds (dto.tcp_io_timeout_s);
	pow_sleep_interval = std::chrono::nanoseconds (dto.pow_sleep_interval_ns);
	external_address = std::string (reinterpret_cast<const char *> (dto.external_address), dto.external_address_len);
	external_port = dto.external_port;
	tcp_incoming_connections_max = dto.tcp_incoming_connections_max;
	use_memory_pools = dto.use_memory_pools;
	confirmation_history_size = dto.confirmation_history_size;
	active_elections_size = dto.active_elections_size;
	active_elections_hinted_limit_percentage = dto.active_elections_hinted_limit_percentage;
	bandwidth_limit = dto.bandwidth_limit;
	bandwidth_limit_burst_ratio = dto.bandwidth_limit_burst_ratio;
	bootstrap_bandwidth_limit = dto.bootstrap_bandwidth_limit;
	bootstrap_bandwidth_burst_ratio = dto.bootstrap_bandwidth_burst_ratio;
	conf_height_processor_batch_min_time = std::chrono::milliseconds (dto.conf_height_processor_batch_min_time_ms);
	backup_before_upgrade = dto.backup_before_upgrade;
	max_work_generate_multiplier = dto.max_work_generate_multiplier;
	frontiers_confirmation = static_cast<nano::frontiers_confirmation_mode> (dto.frontiers_confirmation);
	max_queued_requests = dto.max_queued_requests;
	std::copy (std::begin (dto.rep_crawler_weight_minimum), std::end (dto.rep_crawler_weight_minimum), std::begin (rep_crawler_weight_minimum.bytes));
	work_peers.clear ();
	for (auto i = 0; i < dto.work_peers_count; i++)
	{
		std::string address (reinterpret_cast<const char *> (dto.work_peers[i].address), dto.work_peers[i].address_len);
		work_peers.push_back (std::make_pair (address, dto.work_peers[i].port));
	}
	secondary_work_peers.clear ();
	for (auto i = 0; i < dto.secondary_work_peers_count; i++)
	{
		std::string address (reinterpret_cast<const char *> (dto.secondary_work_peers[i].address), dto.secondary_work_peers[i].address_len);
		secondary_work_peers.push_back (std::make_pair (address, dto.secondary_work_peers[i].port));
	}
	preconfigured_peers.clear ();
	for (auto i = 0; i < dto.preconfigured_peers_count; i++)
	{
		std::string address (reinterpret_cast<const char *> (dto.preconfigured_peers[i].address), dto.preconfigured_peers[i].address_len);
		preconfigured_peers.push_back (address);
	}
	preconfigured_representatives.clear ();
	for (auto i = 0; i < dto.preconfigured_representatives_count; i++)
	{
		nano::account a;
		std::copy (std::begin (dto.preconfigured_representatives[i]), std::end (dto.preconfigured_representatives[i]), std::begin (a.bytes));
		preconfigured_representatives.push_back (a);
	}
	max_pruning_age = std::chrono::seconds (dto.max_pruning_age_s);
	max_pruning_depth = dto.max_pruning_depth;
	callback_address = std::string (reinterpret_cast<const char *> (dto.callback_address), dto.callback_address_len);
	callback_target = std::string (reinterpret_cast<const char *> (dto.callback_target), dto.callback_target_len);
	callback_port = dto.callback_port;
	websocket_config.load_dto (dto.websocket_config);
	ipc_config.load_dto (dto.ipc_config);
	diagnostics_config.load_dto (dto.diagnostics_config);
	stat_config.load_dto (dto.stat_config);
	lmdb_config.load_dto (dto.lmdb_config);
}

nano::error nano::node_config::serialize_toml (nano::tomlconfig & toml) const
{
	auto dto{ to_node_config_dto (*this) };
	if (rsnano::rsn_node_config_serialize_toml (&dto, &toml) < 0)
		return nano::error ("could not TOML serialize node_config");

	return nano::error ();
}

nano::error nano::node_config::deserialize_toml (nano::tomlconfig & toml)
{
	try
	{
		if (toml.has_key ("httpcallback"))
		{
			auto callback_l (toml.get_required_child ("httpcallback"));
			callback_l.get<std::string> ("address", callback_address);
			callback_l.get<uint16_t> ("port", callback_port);
			callback_l.get<std::string> ("target", callback_target);
		}

		if (toml.has_key ("logging"))
		{
			auto logging_l (toml.get_required_child ("logging"));
			logging.deserialize_toml (logging_l);
		}

		if (toml.has_key ("websocket"))
		{
			auto websocket_config_l (toml.get_required_child ("websocket"));
			websocket_config.deserialize_toml (websocket_config_l);
		}

		if (toml.has_key ("ipc"))
		{
			auto ipc_config_l (toml.get_required_child ("ipc"));
			ipc_config.deserialize_toml (ipc_config_l);
		}

		if (toml.has_key ("diagnostics"))
		{
			auto diagnostics_config_l (toml.get_required_child ("diagnostics"));
			diagnostics_config.deserialize_toml (diagnostics_config_l);
		}

		if (toml.has_key ("statistics"))
		{
			auto stat_config_l (toml.get_required_child ("statistics"));
			stat_config.deserialize_toml (stat_config_l);
		}

		if (toml.has_key ("work_peers"))
		{
			work_peers.clear ();
			toml.array_entries_required<std::string> ("work_peers", [this] (std::string const & entry_a) {
				this->deserialize_address (entry_a, this->work_peers);
			});
		}

		if (toml.has_key (preconfigured_peers_key))
		{
			preconfigured_peers.clear ();
			toml.array_entries_required<std::string> (preconfigured_peers_key, [this] (std::string entry) {
				preconfigured_peers.push_back (entry);
			});
		}

		if (toml.has_key ("preconfigured_representatives"))
		{
			preconfigured_representatives.clear ();
			toml.array_entries_required<std::string> ("preconfigured_representatives", [this, &toml] (std::string entry) {
				nano::account representative{};
				if (representative.decode_account (entry))
				{
					toml.get_error ().set ("Invalid representative account: " + entry);
				}
				preconfigured_representatives.push_back (representative);
			});
		}

		if (preconfigured_representatives.empty ())
		{
			toml.get_error ().set ("At least one representative account must be set");
		}

		auto receive_minimum_l (receive_minimum.to_string_dec ());
		if (toml.has_key ("receive_minimum"))
		{
			receive_minimum_l = toml.get<std::string> ("receive_minimum");
		}
		if (receive_minimum.decode_dec (receive_minimum_l))
		{
			toml.get_error ().set ("receive_minimum contains an invalid decimal amount");
		}

		auto online_weight_minimum_l (online_weight_minimum.to_string_dec ());
		if (toml.has_key ("online_weight_minimum"))
		{
			online_weight_minimum_l = toml.get<std::string> ("online_weight_minimum");
		}
		if (online_weight_minimum.decode_dec (online_weight_minimum_l))
		{
			toml.get_error ().set ("online_weight_minimum contains an invalid decimal amount");
		}

		auto vote_minimum_l (vote_minimum.to_string_dec ());
		if (toml.has_key ("vote_minimum"))
		{
			vote_minimum_l = toml.get<std::string> ("vote_minimum");
		}
		if (vote_minimum.decode_dec (vote_minimum_l))
		{
			toml.get_error ().set ("vote_minimum contains an invalid decimal amount");
		}

		auto delay_l = vote_generator_delay.count ();
		toml.get ("vote_generator_delay", delay_l);
		vote_generator_delay = std::chrono::milliseconds (delay_l);

		toml.get<unsigned> ("vote_generator_threshold", vote_generator_threshold);

		auto block_processor_batch_max_time_l = block_processor_batch_max_time.count ();
		toml.get ("block_processor_batch_max_time", block_processor_batch_max_time_l);
		block_processor_batch_max_time = std::chrono::milliseconds (block_processor_batch_max_time_l);

		auto unchecked_cutoff_time_l = static_cast<unsigned long> (unchecked_cutoff_time.count ());
		toml.get ("unchecked_cutoff_time", unchecked_cutoff_time_l);
		unchecked_cutoff_time = std::chrono::seconds (unchecked_cutoff_time_l);

		auto tcp_io_timeout_l = static_cast<unsigned long> (tcp_io_timeout.count ());
		toml.get ("tcp_io_timeout", tcp_io_timeout_l);
		tcp_io_timeout = std::chrono::seconds (tcp_io_timeout_l);

		if (toml.has_key ("peering_port"))
		{
			std::uint16_t peering_port_l{};
			toml.get_required<uint16_t> ("peering_port", peering_port_l);
			peering_port = peering_port_l;
		}

		toml.get<unsigned> ("bootstrap_fraction_numerator", bootstrap_fraction_numerator);
		toml.get<unsigned> ("election_hint_weight_percent", election_hint_weight_percent);
		toml.get<unsigned> ("password_fanout", password_fanout);
		toml.get<unsigned> ("io_threads", io_threads);
		toml.get<unsigned> ("work_threads", work_threads);
		toml.get<unsigned> ("network_threads", network_threads);
		toml.get<unsigned> ("bootstrap_connections", bootstrap_connections);
		toml.get<unsigned> ("bootstrap_connections_max", bootstrap_connections_max);
		toml.get<unsigned> ("bootstrap_initiator_threads", bootstrap_initiator_threads);
		toml.get<unsigned> ("bootstrap_serving_threads", bootstrap_serving_threads);
		toml.get<uint32_t> ("bootstrap_frontier_request_count", bootstrap_frontier_request_count);
		toml.get<bool> ("enable_voting", enable_voting);
		toml.get<bool> ("allow_local_peers", allow_local_peers);
		toml.get<unsigned> (signature_checker_threads_key, signature_checker_threads);

		if (toml.has_key ("lmdb"))
		{
			auto lmdb_config_l (toml.get_required_child ("lmdb"));
			lmdb_config.deserialize_toml (lmdb_config_l);
		}

		boost::asio::ip::address_v6 external_address_l;
		toml.get<boost::asio::ip::address_v6> ("external_address", external_address_l);
		external_address = external_address_l.to_string ();
		toml.get<uint16_t> ("external_port", external_port);
		toml.get<unsigned> ("tcp_incoming_connections_max", tcp_incoming_connections_max);

		auto pow_sleep_interval_l (pow_sleep_interval.count ());
		toml.get (pow_sleep_interval_key, pow_sleep_interval_l);
		pow_sleep_interval = std::chrono::nanoseconds (pow_sleep_interval_l);
		toml.get<bool> ("use_memory_pools", use_memory_pools);
		toml.get<std::size_t> ("confirmation_history_size", confirmation_history_size);
		toml.get<std::size_t> ("active_elections_size", active_elections_size);

		toml.get<std::size_t> ("bandwidth_limit", bandwidth_limit);
		toml.get<double> ("bandwidth_limit_burst_ratio", bandwidth_limit_burst_ratio);

		toml.get<std::size_t> ("bootstrap_bandwidth_limit", bootstrap_bandwidth_limit);
		toml.get<double> ("bootstrap_bandwidth_burst_ratio", bootstrap_bandwidth_burst_ratio);

		toml.get<bool> ("backup_before_upgrade", backup_before_upgrade);

		auto conf_height_processor_batch_min_time_l (conf_height_processor_batch_min_time.count ());
		toml.get ("conf_height_processor_batch_min_time", conf_height_processor_batch_min_time_l);
		conf_height_processor_batch_min_time = std::chrono::milliseconds (conf_height_processor_batch_min_time_l);

		toml.get<double> ("max_work_generate_multiplier", max_work_generate_multiplier);

		toml.get<uint32_t> ("max_queued_requests", max_queued_requests);

		auto rep_crawler_weight_minimum_l (rep_crawler_weight_minimum.to_string_dec ());
		if (toml.has_key ("rep_crawler_weight_minimum"))
		{
			rep_crawler_weight_minimum_l = toml.get<std::string> ("rep_crawler_weight_minimum");
		}
		if (rep_crawler_weight_minimum.decode_dec (rep_crawler_weight_minimum_l))
		{
			toml.get_error ().set ("rep_crawler_weight_minimum contains an invalid decimal amount");
		}

		if (toml.has_key ("frontiers_confirmation"))
		{
			auto frontiers_confirmation_l (toml.get<std::string> ("frontiers_confirmation"));
			frontiers_confirmation = deserialize_frontiers_confirmation (frontiers_confirmation_l);
		}

		if (toml.has_key ("experimental"))
		{
			auto experimental_config_l (toml.get_required_child ("experimental"));
			if (experimental_config_l.has_key ("secondary_work_peers"))
			{
				secondary_work_peers.clear ();
				experimental_config_l.array_entries_required<std::string> ("secondary_work_peers", [this] (std::string const & entry_a) {
					this->deserialize_address (entry_a, this->secondary_work_peers);
				});
			}
			auto max_pruning_age_l (max_pruning_age.count ());
			experimental_config_l.get ("max_pruning_age", max_pruning_age_l);
			max_pruning_age = std::chrono::seconds (max_pruning_age_l);
			experimental_config_l.get<uint64_t> ("max_pruning_depth", max_pruning_depth);
		}

		// Validate ranges
		if (election_hint_weight_percent < 5 || election_hint_weight_percent > 50)
		{
			toml.get_error ().set ("election_hint_weight_percent must be a number between 5 and 50");
		}
		if (password_fanout < 16 || password_fanout > 1024 * 1024)
		{
			toml.get_error ().set ("password_fanout must be a number between 16 and 1048576");
		}
		if (io_threads == 0)
		{
			toml.get_error ().set ("io_threads must be non-zero");
		}
		if (active_elections_size <= 250 && !network_params.network.is_dev_network ())
		{
			toml.get_error ().set ("active_elections_size must be greater than 250");
		}
		if (bandwidth_limit > std::numeric_limits<std::size_t>::max ())
		{
			toml.get_error ().set ("bandwidth_limit unbounded = 0, default = 10485760, max = 18446744073709551615");
		}
		if (vote_generator_threshold < 1 || vote_generator_threshold > 11)
		{
			toml.get_error ().set ("vote_generator_threshold must be a number between 1 and 11");
		}
		if (max_work_generate_multiplier < 1)
		{
			toml.get_error ().set ("max_work_generate_multiplier must be greater than or equal to 1");
		}
		if (frontiers_confirmation == nano::frontiers_confirmation_mode::invalid)
		{
			toml.get_error ().set ("frontiers_confirmation value is invalid (available: always, auto, disabled)");
		}
		if (block_processor_batch_max_time < network_params.node.process_confirmed_interval)
		{
			toml.get_error ().set ((boost::format ("block_processor_batch_max_time value must be equal or larger than %1%ms") % network_params.node.process_confirmed_interval.count ()).str ());
		}
		if (max_pruning_age < std::chrono::seconds (5 * 60) && !network_params.network.is_dev_network ())
		{
			toml.get_error ().set ("max_pruning_age must be greater than or equal to 5 minutes");
		}
		if (bootstrap_frontier_request_count < 1024)
		{
			toml.get_error ().set ("bootstrap_frontier_request_count must be greater than or equal to 1024");
		}
	}
	catch (std::runtime_error const & ex)
	{
		toml.get_error ().set (ex.what ());
	}

	return toml.get_error ();
}

nano::frontiers_confirmation_mode nano::node_config::deserialize_frontiers_confirmation (std::string const & string_a)
{
	if (string_a == "always")
	{
		return nano::frontiers_confirmation_mode::always;
	}
	else if (string_a == "auto")
	{
		return nano::frontiers_confirmation_mode::automatic;
	}
	else if (string_a == "disabled")
	{
		return nano::frontiers_confirmation_mode::disabled;
	}
	else
	{
		return nano::frontiers_confirmation_mode::invalid;
	}
}

void nano::node_config::deserialize_address (std::string const & entry_a, std::vector<std::pair<std::string, uint16_t>> & container_a) const
{
	auto port_position (entry_a.rfind (':'));
	bool result = (port_position == -1);
	if (!result)
	{
		auto port_str (entry_a.substr (port_position + 1));
		uint16_t port;
		result |= parse_port (port_str, port);
		if (!result)
		{
			auto address (entry_a.substr (0, port_position));
			container_a.emplace_back (address, port);
		}
	}
}

nano::account nano::node_config::random_representative () const
{
	debug_assert (!preconfigured_representatives.empty ());
	std::size_t index (nano::random_pool::generate_word32 (0, static_cast<uint32_t> (preconfigured_representatives.size () - 1)));
	auto result (preconfigured_representatives[index]);
	return result;
}

nano::node_flags::node_flags () :
	handle{ rsnano::rsn_node_flags_create () }
{
}

nano::node_flags::node_flags (nano::node_flags && other_a) :
	handle{ other_a.handle }
{
	other_a.handle = nullptr;
}

nano::node_flags::node_flags (const nano::node_flags & other_a) :
	handle{ rsnano::rsn_node_flags_clone (other_a.handle) }
{
}

nano::node_flags::~node_flags ()
{
	if (handle)
		rsnano::rsn_node_flags_destroy (handle);
}

nano::node_flags & nano::node_flags::operator= (nano::node_flags const & other_a)
{
	if (handle != nullptr)
		rsnano::rsn_node_flags_destroy (handle);
	handle = rsnano::rsn_node_flags_clone (other_a.handle);
	return *this;
}

nano::node_flags & nano::node_flags::operator= (nano::node_flags && other_a)
{
	if (handle != nullptr)
		rsnano::rsn_node_flags_destroy (handle);
	handle = other_a.handle;
	other_a.handle = nullptr;
	return *this;
}

rsnano::NodeFlagsDto nano::node_flags::flags_dto () const
{
	rsnano::NodeFlagsDto dto;
	rsnano::rsn_node_flags_get (handle, &dto);
	return dto;
}

void nano::node_flags::set_flag (std::function<void (rsnano::NodeFlagsDto &)> const & callback)
{
	auto dto{ flags_dto () };
	callback (dto);
	rsnano::rsn_node_flags_set (handle, &dto);
}

std::vector<std::string> nano::node_flags::config_overrides () const
{
	std::array<rsnano::StringDto, 1000> overrides;
	auto count = rsnano::rsn_node_flags_config_overrides (handle, overrides.data (), overrides.size ());
	std::vector<std::string> result;
	result.reserve (count);
	for (auto i = 0; i < count; ++i)
	{
		result.push_back (rsnano::convert_dto_to_string (overrides[i]));
	}
	return result;
}

void nano::node_flags::set_config_overrides (const std::vector<std::string> & overrides)
{
	std::vector<int8_t const *> dtos;
	dtos.reserve (overrides.size ());
	for (const auto & s : overrides)
	{
		dtos.push_back (reinterpret_cast<const int8_t *> (s.data ()));
	}
	rsnano::rsn_node_flags_config_set_overrides (handle, dtos.data (), dtos.size ());
}

std::vector<std::string> nano::node_flags::rpc_config_overrides () const
{
	std::array<rsnano::StringDto, 1000> overrides;
	auto count = rsnano::rsn_node_flags_rpc_config_overrides (handle, overrides.data (), overrides.size ());
	std::vector<std::string> result;
	result.reserve (count);
	for (auto i = 0; i < count; ++i)
	{
		result.push_back (rsnano::convert_dto_to_string (overrides[i]));
	}
	return result;
}

void nano::node_flags::set_rpc_overrides (const std::vector<std::string> & overrides)
{
	std::vector<int8_t const *> dtos;
	dtos.reserve (overrides.size ());
	for (const auto & s : overrides)
	{
		dtos.push_back (reinterpret_cast<const int8_t *> (s.data ()));
	}
	rsnano::rsn_node_flags_rpc_config_set_overrides (handle, dtos.data (), dtos.size ());
}

bool nano::node_flags::disable_add_initial_peers () const
{
	return flags_dto ().disable_add_initial_peers;
}

void nano::node_flags::set_disable_add_initial_peers (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_add_initial_peers = value; });
}

bool nano::node_flags::disable_backup () const
{
	return flags_dto ().disable_backup;
}
void nano::node_flags::set_disable_backup (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_backup = value; });
}
bool nano::node_flags::disable_lazy_bootstrap () const
{
	return flags_dto ().disable_lazy_bootstrap;
}
void nano::node_flags::set_disable_lazy_bootstrap (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_lazy_bootstrap = value; });
}
bool nano::node_flags::disable_legacy_bootstrap () const
{
	return flags_dto ().disable_legacy_bootstrap;
}
void nano::node_flags::set_disable_legacy_bootstrap (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_legacy_bootstrap = value; });
}
bool nano::node_flags::disable_wallet_bootstrap () const
{
	return flags_dto ().disable_wallet_bootstrap;
}
void nano::node_flags::set_disable_wallet_bootstrap (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_wallet_bootstrap = value; });
}
bool nano::node_flags::disable_bootstrap_listener () const
{
	return flags_dto ().disable_bootstrap_listener;
}
void nano::node_flags::set_disable_bootstrap_listener (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_bootstrap_listener = value; });
}
bool nano::node_flags::disable_bootstrap_bulk_pull_server () const
{
	return flags_dto ().disable_bootstrap_bulk_pull_server;
}
void nano::node_flags::set_disable_bootstrap_bulk_pull_server (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_bootstrap_bulk_pull_server = value; });
}
bool nano::node_flags::disable_bootstrap_bulk_push_client () const
{
	return flags_dto ().disable_bootstrap_bulk_push_client;
}
void nano::node_flags::set_disable_bootstrap_bulk_push_client (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_bootstrap_bulk_push_client = value; });
}
bool nano::node_flags::disable_ongoing_bootstrap () const // For testing onl
{
	return flags_dto ().disable_ongoing_bootstrap;
}
void nano::node_flags::set_disable_ongoing_bootstrap (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_ongoing_bootstrap = value; });
}
bool nano::node_flags::disable_rep_crawler () const
{
	return flags_dto ().disable_rep_crawler;
}
void nano::node_flags::set_disable_rep_crawler (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_rep_crawler = value; });
}
bool nano::node_flags::disable_request_loop () const // For testing onl
{
	return flags_dto ().disable_request_loop;
}
void nano::node_flags::set_disable_request_loop (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_request_loop = value; });
}
bool nano::node_flags::disable_tcp_realtime () const
{
	return flags_dto ().disable_tcp_realtime;
}
void nano::node_flags::set_disable_tcp_realtime (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_tcp_realtime = value; });
}
bool nano::node_flags::disable_udp () const
{
	return flags_dto ().disable_udp;
}
void nano::node_flags::set_disable_udp (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_udp = value; });
}
bool nano::node_flags::disable_unchecked_cleanup () const
{
	return flags_dto ().disable_unchecked_cleanup;
}
void nano::node_flags::set_disable_unchecked_cleanup (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_unchecked_cleanup = value; });
}
bool nano::node_flags::disable_unchecked_drop () const
{
	return flags_dto ().disable_unchecked_drop;
}
void nano::node_flags::set_disable_unchecked_drop (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_unchecked_drop = value; });
}
bool nano::node_flags::disable_providing_telemetry_metrics () const
{
	return flags_dto ().disable_providing_telemetry_metrics;
}
void nano::node_flags::set_disable_providing_telemetry_metrics (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_providing_telemetry_metrics = value; });
}
bool nano::node_flags::disable_ongoing_telemetry_requests () const
{
	return flags_dto ().disable_ongoing_telemetry_requests;
}
void nano::node_flags::set_disable_ongoing_telemetry_requests (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_ongoing_telemetry_requests = value; });
}
bool nano::node_flags::disable_initial_telemetry_requests () const
{
	return flags_dto ().disable_initial_telemetry_requests;
}
void nano::node_flags::set_disable_initial_telemetry_requests (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_initial_telemetry_requests = value; });
}
bool nano::node_flags::disable_block_processor_unchecked_deletion () const
{
	return flags_dto ().disable_block_processor_unchecked_deletion;
}
void nano::node_flags::set_disable_block_processor_unchecked_deletion (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_block_processor_unchecked_deletion = value; });
}
bool nano::node_flags::disable_block_processor_republishing () const
{
	return flags_dto ().disable_block_processor_republishing;
}
void nano::node_flags::set_disable_block_processor_republishing (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_block_processor_republishing = value; });
}
bool nano::node_flags::allow_bootstrap_peers_duplicates () const
{
	return flags_dto ().allow_bootstrap_peers_duplicates;
}
void nano::node_flags::set_allow_bootstrap_peers_duplicates (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.allow_bootstrap_peers_duplicates = value; });
}
bool nano::node_flags::disable_max_peers_per_ip () const // For testing onl
{
	return flags_dto ().disable_max_peers_per_ip;
}
void nano::node_flags::set_disable_max_peers_per_ip (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_max_peers_per_ip = value; });
}
bool nano::node_flags::disable_max_peers_per_subnetwork () const // For testing onl
{
	return flags_dto ().disable_max_peers_per_subnetwork;
}
void nano::node_flags::set_disable_max_peers_per_subnetwork (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_max_peers_per_subnetwork = value; });
}
bool nano::node_flags::force_use_write_database_queue () const // For testing only
{
	return flags_dto ().force_use_write_database_queue;
}
void nano::node_flags::set_force_use_write_database_queue (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.force_use_write_database_queue = value; });
}
bool nano::node_flags::disable_search_pending () const // For testing only
{
	return flags_dto ().disable_search_pending;
}
void nano::node_flags::set_disable_search_pending (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_search_pending = value; });
}
bool nano::node_flags::enable_pruning () const
{
	return flags_dto ().enable_pruning;
}
void nano::node_flags::set_enable_pruning (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.enable_pruning = value; });
}
bool nano::node_flags::fast_bootstrap () const
{
	return flags_dto ().fast_bootstrap;
}
void nano::node_flags::set_fast_bootstrap (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.fast_bootstrap = value; });
}
bool nano::node_flags::read_only () const
{
	return flags_dto ().read_only;
}
void nano::node_flags::set_read_only (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.read_only = value; });
}
bool nano::node_flags::disable_connection_cleanup () const
{
	return flags_dto ().disable_connection_cleanup;
}
void nano::node_flags::set_disable_connection_cleanup (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.disable_connection_cleanup = value; });
}
nano::confirmation_height_mode nano::node_flags::confirmation_height_processor_mode () const
{
	return static_cast<confirmation_height_mode> (flags_dto ().confirmation_height_processor_mode);
}
void nano::node_flags::set_confirmation_height_processor_mode (nano::confirmation_height_mode mode)
{
	auto dto{ flags_dto () };
	dto.confirmation_height_processor_mode = static_cast<uint8_t> (mode);
	rsnano::rsn_node_flags_set (handle, &dto);
}
nano::generate_cache nano::node_flags::generate_cache () const
{
	return nano::generate_cache{ rsnano::rsn_node_flags_generate_cache (handle) };
}
void nano::node_flags::set_generate_cache (nano::generate_cache const & cache)
{
	rsnano::rsn_node_flags_generate_set_cache (handle, cache.handle);
}
bool nano::node_flags::inactive_node () const
{
	return flags_dto ().inactive_node;
}
void nano::node_flags::set_inactive_node (bool value)
{
	set_flag ([value] (rsnano::NodeFlagsDto & dto) { dto.inactive_node = value; });
}
std::size_t nano::node_flags::block_processor_batch_size () const
{
	return flags_dto ().block_processor_batch_size;
}
void nano::node_flags::set_block_processor_batch_size (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.block_processor_batch_size = size; });
}
std::size_t nano::node_flags::block_processor_full_size () const
{
	return flags_dto ().block_processor_full_size;
}
void nano::node_flags::set_block_processor_full_size (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.block_processor_full_size = size; });
}
std::size_t nano::node_flags::block_processor_verification_size () const
{
	return flags_dto ().block_processor_verification_size;
}
void nano::node_flags::set_block_processor_verification_size (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.block_processor_verification_size = size; });
}
std::size_t nano::node_flags::inactive_votes_cache_size () const
{
	return flags_dto ().inactive_votes_cache_size;
}
void nano::node_flags::set_inactive_votes_cache_size (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.inactive_votes_cache_size = size; });
}
std::size_t nano::node_flags::vote_processor_capacity () const
{
	return flags_dto ().vote_processor_capacity;
}
void nano::node_flags::set_vote_processor_capacity (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.vote_processor_capacity = size; });
}
std::size_t nano::node_flags::bootstrap_interval () const
{
	return flags_dto ().bootstrap_interval;
}
void nano::node_flags::set_bootstrap_interval (std::size_t size)
{
	set_flag ([size] (rsnano::NodeFlagsDto & dto) { dto.bootstrap_interval = size; });
}
