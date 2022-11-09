#include <nano/lib/stats.hpp>
#include <nano/lib/threading.hpp>
#include <nano/lib/utility.hpp>
#include <nano/node/network.hpp>
#include <nano/node/nodeconfig.hpp>
#include <nano/node/transport/inproc.hpp>
#include <nano/node/vote_processor.hpp>
#include <nano/node/voting.hpp>
#include <nano/node/wallet.hpp>
#include <nano/secure/ledger.hpp>
#include <nano/secure/store.hpp>

#include <chrono>

nano::vote_spacing::vote_spacing (std::chrono::milliseconds const & delay) :
	handle{ rsnano::rsn_vote_spacing_create (delay.count ()) }
{
}

nano::vote_spacing::~vote_spacing ()
{
	rsnano::rsn_vote_spacing_destroy (handle);
}

bool nano::vote_spacing::votable (nano::root const & root_a, nano::block_hash const & hash_a) const
{
	return rsnano::rsn_vote_spacing_votable (handle, root_a.bytes.data (), hash_a.bytes.data ());
}

void nano::vote_spacing::flag (nano::root const & root_a, nano::block_hash const & hash_a)
{
	rsnano::rsn_vote_spacing_flag (handle, root_a.bytes.data (), hash_a.bytes.data ());
}

std::size_t nano::vote_spacing::size () const
{
	return rsnano::rsn_vote_spacing_len (handle);
}

nano::local_vote_history::local_vote_history (nano::voting_constants const & constants) :
	handle{ rsnano::rsn_local_vote_history_create (constants.max_cache) }
{
}

nano::local_vote_history::~local_vote_history ()
{
	rsnano::rsn_local_vote_history_destroy (handle);
}

void nano::local_vote_history::add (nano::root const & root_a, nano::block_hash const & hash_a, std::shared_ptr<nano::vote> const & vote_a)
{
	rsnano::rsn_local_vote_history_add (handle, root_a.bytes.data (), hash_a.bytes.data (), vote_a->get_handle ());
}

void nano::local_vote_history::erase (nano::root const & root_a)
{
	rsnano::rsn_local_vote_history_erase (handle, root_a.bytes.data ());
}

class LocalVotesResultWrapper
{
public:
	LocalVotesResultWrapper () :
		result{}
	{
	}
	~LocalVotesResultWrapper ()
	{
		rsnano::rsn_local_vote_history_votes_destroy (result.handle);
	}
	rsnano::LocalVotesResult result;
};

std::vector<std::shared_ptr<nano::vote>> nano::local_vote_history::votes (nano::root const & root_a, nano::block_hash const & hash_a, bool const is_final_a) const
{
	LocalVotesResultWrapper result_wrapper;
	rsnano::rsn_local_vote_history_votes (handle, root_a.bytes.data (), hash_a.bytes.data (), is_final_a, &result_wrapper.result);
	std::vector<std::shared_ptr<nano::vote>> votes;
	votes.reserve (result_wrapper.result.count);
	for (auto i (0); i < result_wrapper.result.count; ++i)
	{
		votes.push_back (std::make_shared<nano::vote> (result_wrapper.result.votes[i]));
	}
	return votes;
}

bool nano::local_vote_history::exists (nano::root const & root_a) const
{
	return rsnano::rsn_local_vote_history_exists (handle, root_a.bytes.data ());
}

std::size_t nano::local_vote_history::size () const
{
	return rsnano::rsn_local_vote_history_size (handle);
}

std::unique_ptr<nano::container_info_component> nano::collect_container_info (nano::local_vote_history & history, std::string const & name)
{
	std::size_t sizeof_element;
	std::size_t history_count;
	rsnano::rsn_local_vote_history_container_info (history.handle, &sizeof_element, &history_count);
	auto composite = std::make_unique<container_info_composite> (name);
	/* This does not currently loop over each element inside the cache to get the sizes of the votes inside history*/
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "history", history_count, sizeof_element }));
	return composite;
}

nano::vote_broadcaster::vote_broadcaster (nano::vote_processor & vote_processor_a, nano::network & network_a) :
	vote_processor{ vote_processor_a },
	network{ network_a }
{
}

void nano::vote_broadcaster::broadcast (std::shared_ptr<nano::vote> const & vote_a) const
{
	network.flood_vote_pr (vote_a);
	network.flood_vote (vote_a, 2.0f);
	vote_processor.vote (vote_a, std::make_shared<nano::transport::inproc::channel> (network.node, network.node));
}

nano::vote_generator::vote_generator (nano::node_config const & config_a, nano::ledger & ledger_a, nano::wallets & wallets_a, nano::vote_processor & vote_processor_a, nano::local_vote_history & history_a, nano::network & network_a, nano::stat & stats_a, bool is_final_a) :
	config (config_a),
	ledger (ledger_a),
	wallets (wallets_a),
	history (history_a),
	spacing{ config_a.network_params.voting.delay },
	vote_broadcaster{ vote_processor_a, network_a },
	stats (stats_a),
	is_final (is_final_a),
	vote_generation_queue{ stats, nano::stat::type::vote_generator, nano::thread_role::name::vote_generator_queue, /* single threaded */ 1, /* max queue size */ 1024 * 32, /* max batch size */ 1024 * 4 }
{
	vote_generation_queue.process_batch = [this] (auto & batch) {
		process_batch (batch);
	};
}

nano::vote_generator::~vote_generator ()
{
	stop ();
}

void nano::vote_generator::process (nano::write_transaction const & transaction, nano::root const & root_a, nano::block_hash const & hash_a)
{
	auto cached_votes (history.votes (root_a, hash_a, is_final));
	if (!cached_votes.empty ())
	{
		for (auto const & vote : cached_votes)
		{
			vote_broadcaster.broadcast (vote);
		}
	}
	else
	{
		auto should_vote (false);
		if (is_final)
		{
			auto block (ledger.store.block ().get (transaction, hash_a));
			should_vote = block != nullptr && ledger.dependents_confirmed (transaction, *block) && ledger.store.final_vote ().put (transaction, block->qualified_root (), hash_a);
			debug_assert (block == nullptr || root_a == block->root ());
		}
		else
		{
			auto block (ledger.store.block ().get (transaction, hash_a));
			should_vote = block != nullptr && ledger.dependents_confirmed (transaction, *block);
		}
		if (should_vote)
		{
			nano::unique_lock<nano::mutex> lock (mutex);
			candidates.emplace_back (root_a, hash_a);
			if (candidates.size () >= nano::network::confirm_ack_hashes_max)
			{
				lock.unlock ();
				condition.notify_all ();
			}
		}
	}
}

void nano::vote_generator::start ()
{
	debug_assert (!thread.joinable ());
	thread = std::thread ([this] () { run (); });

	vote_generation_queue.start ();
}

void nano::vote_generator::stop ()
{
	vote_generation_queue.stop ();

	nano::unique_lock<nano::mutex> lock (mutex);
	stopped = true;

	lock.unlock ();
	condition.notify_all ();

	if (thread.joinable ())
	{
		thread.join ();
	}
}

void nano::vote_generator::add (const root & root, const block_hash & hash)
{
	vote_generation_queue.add (std::make_pair (root, hash));
}

void nano::vote_generator::process_batch (std::deque<queue_entry_t> & batch)
{
	auto transaction = ledger.store.tx_begin_write ({ tables::final_votes });

	for (auto & [root, hash] : batch)
	{
		process (*transaction, root, hash);
	}
}

std::size_t nano::vote_generator::generate (std::vector<std::shared_ptr<nano::block>> const & blocks_a, std::shared_ptr<nano::transport::channel> const & channel_a)
{
	request_t::first_type req_candidates;
	{
		auto transaction (ledger.store.tx_begin_read ());
		auto dependents_confirmed = [&transaction, this] (auto const & block_a) {
			return this->ledger.dependents_confirmed (*transaction, *block_a);
		};
		auto as_candidate = [] (auto const & block_a) {
			return candidate_t{ block_a->root (), block_a->hash () };
		};
		nano::transform_if (blocks_a.begin (), blocks_a.end (), std::back_inserter (req_candidates), dependents_confirmed, as_candidate);
	}
	auto const result = req_candidates.size ();
	nano::lock_guard<nano::mutex> guard (mutex);
	requests.emplace_back (std::move (req_candidates), channel_a);
	while (requests.size () > max_requests)
	{
		// On a large queue of requests, erase the oldest one
		requests.pop_front ();
		stats.inc (nano::stat::type::vote_generator, nano::stat::detail::generator_replies_discarded);
	}
	return result;
}

void nano::vote_generator::set_reply_action (std::function<void (std::shared_ptr<nano::vote> const &, std::shared_ptr<nano::transport::channel> const &)> action_a)
{
	release_assert (!reply_action);
	reply_action = action_a;
}

void nano::vote_generator::broadcast (nano::unique_lock<nano::mutex> & lock_a)
{
	debug_assert (lock_a.owns_lock ());
	std::unordered_set<std::shared_ptr<nano::vote>> cached_sent;
	std::vector<nano::block_hash> hashes;
	std::vector<nano::root> roots;
	hashes.reserve (nano::network::confirm_ack_hashes_max);
	roots.reserve (nano::network::confirm_ack_hashes_max);
	while (!candidates.empty () && hashes.size () < nano::network::confirm_ack_hashes_max)
	{
		auto const & [root, hash] = candidates.front ();
		auto cached_votes = history.votes (root, hash, is_final);
		for (auto const & cached_vote : cached_votes)
		{
			if (cached_sent.insert (cached_vote).second)
			{
				vote_broadcaster.broadcast (cached_vote);
			}
		}
		if (cached_votes.empty () && std::find (roots.begin (), roots.end (), root) == roots.end ())
		{
			if (spacing.votable (root, hash))
			{
				roots.push_back (root);
				hashes.push_back (hash);
			}
			else
			{
				stats.inc (nano::stat::type::vote_generator, nano::stat::detail::generator_spacing);
			}
		}
		candidates.pop_front ();
	}
	if (!hashes.empty ())
	{
		lock_a.unlock ();
		vote (hashes, roots, [this] (auto const & vote_a) {
			this->vote_broadcaster.broadcast (vote_a);
			this->stats.inc (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts);
		});
		lock_a.lock ();
	}
}

void nano::vote_generator::reply (nano::unique_lock<nano::mutex> & lock_a, request_t && request_a)
{
	lock_a.unlock ();
	std::unordered_set<std::shared_ptr<nano::vote>> cached_sent;
	auto i (request_a.first.cbegin ());
	auto n (request_a.first.cend ());
	while (i != n && !stopped)
	{
		std::vector<nano::block_hash> hashes;
		std::vector<nano::root> roots;
		hashes.reserve (nano::network::confirm_ack_hashes_max);
		roots.reserve (nano::network::confirm_ack_hashes_max);
		for (; i != n && hashes.size () < nano::network::confirm_ack_hashes_max; ++i)
		{
			auto const & [root, hash] = *i;
			auto cached_votes = history.votes (root, hash, is_final);
			for (auto const & cached_vote : cached_votes)
			{
				if (cached_sent.insert (cached_vote).second)
				{
					stats.add (nano::stat::type::requests, nano::stat::detail::requests_cached_late_hashes, stat::dir::in, cached_vote->hashes ().size ());
					stats.inc (nano::stat::type::requests, nano::stat::detail::requests_cached_late_votes, stat::dir::in);
					reply_action (cached_vote, request_a.second);
				}
			}
			if (cached_votes.empty () && std::find (roots.begin (), roots.end (), root) == roots.end ())
			{
				if (spacing.votable (root, hash))
				{
					roots.push_back (root);
					hashes.push_back (hash);
				}
				else
				{
					stats.inc (nano::stat::type::vote_generator, nano::stat::detail::generator_spacing);
				}
			}
		}
		if (!hashes.empty ())
		{
			stats.add (nano::stat::type::requests, nano::stat::detail::requests_generated_hashes, stat::dir::in, hashes.size ());
			vote (hashes, roots, [this, &channel = request_a.second] (std::shared_ptr<nano::vote> const & vote_a) {
				this->reply_action (vote_a, channel);
				this->stats.inc (nano::stat::type::requests, nano::stat::detail::requests_generated_votes, stat::dir::in);
			});
		}
	}
	stats.inc (nano::stat::type::vote_generator, nano::stat::detail::generator_replies);
	lock_a.lock ();
}

void nano::vote_generator::vote (std::vector<nano::block_hash> const & hashes_a, std::vector<nano::root> const & roots_a, std::function<void (std::shared_ptr<nano::vote> const &)> const & action_a)
{
	debug_assert (hashes_a.size () == roots_a.size ());
	std::vector<std::shared_ptr<nano::vote>> votes_l;
	wallets.foreach_representative ([this, &hashes_a, &votes_l] (nano::public_key const & pub_a, nano::raw_key const & prv_a) {
		auto timestamp = this->is_final ? nano::vote::timestamp_max : nano::milliseconds_since_epoch ();
		uint8_t duration = this->is_final ? nano::vote::duration_max : /*8192ms*/ 0x9;
		votes_l.emplace_back (std::make_shared<nano::vote> (pub_a, prv_a, timestamp, duration, hashes_a));
	});
	for (auto const & vote_l : votes_l)
	{
		for (std::size_t i (0), n (hashes_a.size ()); i != n; ++i)
		{
			history.add (roots_a[i], hashes_a[i], vote_l);
			spacing.flag (roots_a[i], hashes_a[i]);
		}
		action_a (vote_l);
	}
}

void nano::vote_generator::run ()
{
	nano::thread_role::set (nano::thread_role::name::voting);
	nano::unique_lock<nano::mutex> lock (mutex);
	while (!stopped)
	{
		if (candidates.size () >= nano::network::confirm_ack_hashes_max)
		{
			broadcast (lock);
		}
		else if (!requests.empty ())
		{
			auto request (requests.front ());
			requests.pop_front ();
			reply (lock, std::move (request));
		}
		else
		{
			condition.wait_for (lock, config.vote_generator_delay, [this] () { return this->candidates.size () >= nano::network::confirm_ack_hashes_max; });
			if (candidates.size () >= config.vote_generator_threshold && candidates.size () < nano::network::confirm_ack_hashes_max)
			{
				condition.wait_for (lock, config.vote_generator_delay, [this] () { return this->candidates.size () >= nano::network::confirm_ack_hashes_max; });
			}
			if (!candidates.empty ())
			{
				broadcast (lock);
			}
		}
	}
}

nano::vote_generator_session::vote_generator_session (nano::vote_generator & vote_generator_a) :
	generator (vote_generator_a)
{
}

void nano::vote_generator_session::add (nano::root const & root_a, nano::block_hash const & hash_a)
{
	debug_assert (nano::thread_role::get () == nano::thread_role::name::request_loop);
	items.emplace_back (root_a, hash_a);
}

void nano::vote_generator_session::flush ()
{
	debug_assert (nano::thread_role::get () == nano::thread_role::name::request_loop);
	for (auto const & [root, hash] : items)
	{
		generator.add (root, hash);
	}
}

std::unique_ptr<nano::container_info_component> nano::collect_container_info (nano::vote_generator & vote_generator, std::string const & name)
{
	std::size_t candidates_count = 0;
	std::size_t requests_count = 0;
	{
		nano::lock_guard<nano::mutex> guard (vote_generator.mutex);
		candidates_count = vote_generator.candidates.size ();
		requests_count = vote_generator.requests.size ();
	}
	auto sizeof_candidate_element = sizeof (decltype (vote_generator.candidates)::value_type);
	auto sizeof_request_element = sizeof (decltype (vote_generator.requests)::value_type);
	auto composite = std::make_unique<container_info_composite> (name);
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "candidates", candidates_count, sizeof_candidate_element }));
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "requests", requests_count, sizeof_request_element }));
	composite->add_component (vote_generator.vote_generation_queue.collect_container_info ("vote_generation_queue"));
	return composite;
}
