#include "nano/lib/rsnanoutils.hpp"

#include <nano/lib/stats.hpp>
#include <nano/node/confirmation_height_unbounded.hpp>
#include <nano/node/logging.hpp>
#include <nano/node/write_database_queue.hpp>
#include <nano/secure/ledger.hpp>

#include <boost/format.hpp>

#include <numeric>

namespace
{
void notify_observers_callback_wrapper (void * context, rsnano::BlockHandle * const * block_handles, size_t len)
{
	auto fn = static_cast<std::function<void (std::vector<std::shared_ptr<nano::block>> const &)> *> (context);
	std::vector<std::shared_ptr<nano::block>> blocks;
	for (int i = 0; i < len; ++i)
	{
		blocks.push_back (nano::block_handle_to_block (rsnano::rsn_block_clone (block_handles[i])));
	}

	(*fn) (blocks);
}

void drop_notify_observers_callback (void * context)
{
	auto fn = static_cast<std::function<void (std::vector<std::shared_ptr<nano::block>> const &)> *> (context);
	delete fn;
}
}

nano::confirmation_height_unbounded::confirmation_height_unbounded (nano::ledger & ledger_a, nano::stat & stats_a, nano::write_database_queue & write_database_queue_a, std::chrono::milliseconds batch_separate_pending_min_time_a, nano::logging const & logging_a, std::shared_ptr<nano::logger_mt> & logger_a, uint64_t & batch_write_size_a, std::function<void (std::vector<std::shared_ptr<nano::block>> const &)> const & notify_observers_callback_a, std::function<void (nano::block_hash const &)> const & notify_block_already_cemented_observers_callback_a, std::function<uint64_t ()> const & awaiting_processing_size_callback_a) :
	ledger (ledger_a),
	stats (stats_a),
	write_database_queue (write_database_queue_a),
	logging (logging_a),
	logger (logger_a),
	batch_write_size (batch_write_size_a),
	notify_observers_callback (notify_observers_callback_a),
	notify_block_already_cemented_observers_callback (notify_block_already_cemented_observers_callback_a),
	awaiting_processing_size_callback (awaiting_processing_size_callback_a)
{
	auto logging_dto{ logging_a.to_dto () };
	handle = rsnano::rsn_conf_height_unbounded_create (ledger_a.handle, nano::to_logger_handle (logger_a), &logging_dto, stats_a.handle,
	static_cast<uint64_t> (batch_separate_pending_min_time_a.count ()),
	notify_observers_callback_wrapper,
	new std::function<void (std::vector<std::shared_ptr<nano::block>> const &)> (notify_observers_callback),
	drop_notify_observers_callback);
}

nano::confirmation_height_unbounded::~confirmation_height_unbounded ()
{
	rsnano::rsn_conf_height_unbounded_destroy (handle);
}

void nano::confirmation_height_unbounded::process (std::shared_ptr<nano::block> original_block)
{
	if (pending_empty ())
	{
		clear_process_vars ();
		rsnano::rsn_conf_height_unbounded_restart_timer (handle);
	}
	conf_height_details_shared_ptr receive_details;
	auto current = original_block->hash ();
	std::vector<nano::block_hash> orig_block_callback_data;

	nano::confirmation_height_unbounded::receive_source_pair_vec receive_source_pairs;

	bool first_iter = true;
	auto read_transaction (ledger.store.tx_begin_read ());

	do
	{
		if (!receive_source_pairs.empty ())
		{
			receive_details = receive_source_pairs.back ().receive_details ();
			current = receive_source_pairs.back ().source_hash ();
		}
		else
		{
			// If receive_details is set then this is the final iteration and we are back to the original chain.
			// We need to confirm any blocks below the original hash (incl self) and the first receive block
			// (if the original block is not already a receive)
			if (!receive_details.is_null ())
			{
				current = original_block->hash ();
				receive_details.destroy ();
			}
		}

		std::shared_ptr<nano::block> block;
		if (first_iter)
		{
			debug_assert (current == original_block->hash ());
			// This is the original block passed so can use it directly
			block = original_block;
			rsnano::rsn_conf_height_unbounded_cache_block (handle, original_block->get_handle ());
		}
		else
		{
			block = get_block_and_sideband (current, *read_transaction);
		}
		if (!block)
		{
			auto error_str = (boost::format ("Ledger mismatch trying to set confirmation height for block %1% (unbounded processor)") % current.to_string ()).str ();
			logger->always_log (error_str);
			std::cerr << error_str << std::endl;
		}
		release_assert (block);

		nano::account account (block->account ());
		if (account.is_zero ())
		{
			account = block->sideband ().account ();
		}

		auto block_height = block->sideband ().height ();
		uint64_t confirmation_height = 0;
		rsnano::ConfirmedIteratedPairsIteratorDto account_it;
		rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_find (handle, account.bytes.data (), &account_it);
		if (!account_it.is_end)
		{
			confirmation_height = account_it.confirmed_height;
		}
		else
		{
			nano::confirmation_height_info confirmation_height_info;
			ledger.store.confirmation_height ().get (*read_transaction, account, confirmation_height_info);
			confirmation_height = confirmation_height_info.height ();

			// This block was added to the confirmation height processor but is already confirmed
			if (first_iter && confirmation_height >= block_height)
			{
				debug_assert (current == original_block->hash ());
				notify_block_already_cemented_observers_callback (original_block->hash ());
			}
		}
		auto iterated_height = confirmation_height;
		if (!account_it.is_end && account_it.iterated_height > iterated_height)
		{
			iterated_height = account_it.iterated_height;
		}

		auto count_before_receive = receive_source_pairs.size ();
		std::vector<nano::block_hash> block_callback_datas_required;
		auto already_traversed = iterated_height >= block_height;
		if (!already_traversed)
		{
			collect_unconfirmed_receive_and_sources_for_account (
			block_height, iterated_height, block, current, account,
			*read_transaction, receive_source_pairs, block_callback_datas_required,
			orig_block_callback_data, original_block);
		}

		// Exit early when the processor has been stopped, otherwise this function may take a
		// while (and hence keep the process running) if updating a long chain.
		if (stopped)
		{
			break;
		}

		// No longer need the read transaction
		read_transaction->reset ();

		// If this adds no more open or receive blocks, then we can now confirm this account as well as the linked open/receive block
		// Collect as pending any writes to the database and do them in bulk after a certain time.
		auto confirmed_receives_pending = (count_before_receive != receive_source_pairs.size ());
		if (!confirmed_receives_pending)
		{
			preparation_data preparation_data{ block_height, confirmation_height, iterated_height, account_it, account, receive_details, already_traversed, current, block_callback_datas_required, orig_block_callback_data };
			prepare_iterated_blocks_for_cementing (preparation_data);

			if (!receive_source_pairs.empty ())
			{
				// Pop from the end
				receive_source_pairs.pop ();
			}
		}
		else if (block_height > iterated_height)
		{
			if (!account_it.is_end)
			{
				rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_set_iterated_height (handle, &account_it.account[0], block_height);
			}
			else
			{
				rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_insert (handle, account.bytes.data (), confirmation_height, block_height);
			}
		}

		auto max_write_size_reached = (rsnano::rsn_conf_height_unbounded_pending_writes_size (handle) >= confirmation_height::unbounded_cutoff);
		// When there are a lot of pending confirmation height blocks, it is more efficient to
		// bulk some of them up to enable better write performance which becomes the bottleneck.
		auto min_time_exceeded = rsnano::rsn_conf_height_unbounded_min_time_exceeded (handle);
		auto finished_iterating = receive_source_pairs.empty ();
		auto no_pending = awaiting_processing_size_callback () == 0;
		auto should_output = finished_iterating && (no_pending || min_time_exceeded);

		auto total_pending_write_block_count = rsnano::rsn_conf_height_unbounded_total_pending_write_block_count (handle);
		auto force_write = total_pending_write_block_count > batch_write_size;

		if ((max_write_size_reached || should_output || force_write) && rsnano::rsn_conf_height_unbounded_pending_writes_size (handle) > 0)
		{
			if (write_database_queue.process (nano::writer::confirmation_height))
			{
				auto scoped_write_guard = write_database_queue.pop ();
				cement_blocks (scoped_write_guard);
			}
			else if (force_write)
			{
				// Unbounded processor has grown too large, force a write
				auto scoped_write_guard = write_database_queue.wait (nano::writer::confirmation_height);
				cement_blocks (scoped_write_guard);
			}
		}

		first_iter = false;
		read_transaction->renew ();
	} while ((!receive_source_pairs.empty () || current != original_block->hash ()) && !stopped);
}

void nano::confirmation_height_unbounded::collect_unconfirmed_receive_and_sources_for_account (
uint64_t block_height_a,
uint64_t confirmation_height_a,
std::shared_ptr<nano::block> const & block_a,
nano::block_hash const & hash_a,
nano::account const & account_a,
nano::read_transaction const & transaction_a,
nano::confirmation_height_unbounded::receive_source_pair_vec & receive_source_pairs_a,
std::vector<nano::block_hash> & block_callback_data_a,
std::vector<nano::block_hash> & orig_block_callback_data_a,
std::shared_ptr<nano::block> original_block)
{
	debug_assert (block_a->hash () == hash_a);
	auto hash (hash_a);
	auto num_to_confirm = block_height_a - confirmation_height_a;

	// Handle any sends above a receive
	auto is_original_block = (hash == original_block->hash ());
	auto hit_receive = false;
	auto first_iter = true;
	while ((num_to_confirm > 0) && !hash.is_zero () && !stopped)
	{
		std::shared_ptr<nano::block> block;
		if (first_iter)
		{
			debug_assert (hash == hash_a);
			block = block_a;
			rsnano::rsn_conf_height_unbounded_cache_block (handle, block_a->get_handle ());
		}
		else
		{
			block = get_block_and_sideband (hash, transaction_a);
		}

		if (block)
		{
			auto source (block->source ());
			if (source.is_zero ())
			{
				source = block->link ().as_block_hash ();
			}

			if (!source.is_zero () && !ledger.is_epoch_link (source) && ledger.store.block ().exists (transaction_a, source))
			{
				if (!hit_receive && !block_callback_data_a.empty ())
				{
					// Add the callbacks to the associated receive to retrieve later
					debug_assert (!receive_source_pairs_a.empty ());
					auto last_receive_details = receive_source_pairs_a.back ().receive_details ();
					last_receive_details.set_source_block_callback_data (block_callback_data_a);
					block_callback_data_a.clear ();
				}

				is_original_block = false;
				hit_receive = true;

				auto block_height = confirmation_height_a + num_to_confirm;
				conf_height_details details (account_a, hash, block_height, 1, std::vector<nano::block_hash>{ hash });
				auto shared_details = rsnano::rsn_conf_height_details_shared_ptr_create (details.handle);
				receive_source_pairs_a.push (nano::confirmation_height_unbounded::receive_source_pair{ shared_details, source });
			}
			else if (is_original_block)
			{
				orig_block_callback_data_a.push_back (hash);
			}
			else
			{
				if (!hit_receive)
				{
					// This block is cemented via a recieve, as opposed to below a receive being cemented
					block_callback_data_a.push_back (hash);
				}
				else
				{
					// We have hit a receive before, add the block to it
					auto last_receive_details = receive_source_pairs_a.back ().receive_details ();
					last_receive_details.set_num_blocks_confirmed (last_receive_details.get_num_blocks_confirmed () + 1);
					last_receive_details.add_block_callback_data (hash);

					rsnano::rsn_conf_height_unbounded_implicit_receive_cemented_mapping_add (handle, hash.bytes.data (), last_receive_details.handle);
				}
			}

			hash = block->previous ();
		}

		--num_to_confirm;
		first_iter = false;
	}
}

void nano::confirmation_height_unbounded::prepare_iterated_blocks_for_cementing (preparation_data & preparation_data_a)
{
	auto receive_details = preparation_data_a.receive_details;
	auto block_height = preparation_data_a.block_height;
	if (block_height > preparation_data_a.confirmation_height)
	{
		// Check whether the previous block has been seen. If so, the rest of sends below have already been seen so don't count them
		if (!preparation_data_a.account_it.is_end)
		{
			rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_set_confirmed_height (handle, &preparation_data_a.account_it.account[0], block_height);
			if (block_height > preparation_data_a.iterated_height)
			{
				rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_set_iterated_height (handle, &preparation_data_a.account_it.account[0], block_height);
			}
		}
		else
		{
			rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_insert (handle, preparation_data_a.account.bytes.data (), block_height, block_height);
		}

		auto num_blocks_confirmed = block_height - preparation_data_a.confirmation_height;
		auto block_callback_data = preparation_data_a.block_callback_data;
		if (block_callback_data.empty ())
		{
			if (receive_details.is_null ())
			{
				block_callback_data = preparation_data_a.orig_block_callback_data;
			}
			else
			{
				if (preparation_data_a.already_traversed && receive_details.get_source_block_callback_data ().empty ())
				{
					// We are confirming a block which has already been traversed and found no associated receive details for it.
					conf_height_details_weak_ptr above_receive_details_w{ rsnano::rsn_conf_height_unbounded_get_implicit_receive_cemented (handle, preparation_data_a.current.bytes.data ()) };
					debug_assert (!above_receive_details_w.expired ());
					auto above_receive_details = above_receive_details_w.upgrade ();

					auto num_blocks_already_confirmed = above_receive_details.get_num_blocks_confirmed () - (above_receive_details.get_height () - preparation_data_a.confirmation_height);

					auto block_data{ above_receive_details.get_block_callback_data () };
					auto end_it = block_data.begin () + block_data.size () - (num_blocks_already_confirmed);
					auto start_it = end_it - num_blocks_confirmed;

					block_callback_data.assign (start_it, end_it);
				}
				else
				{
					block_callback_data = receive_details.get_source_block_callback_data ();
				}

				auto num_to_remove = block_callback_data.size () - num_blocks_confirmed;
				block_callback_data.erase (std::next (block_callback_data.rbegin (), num_to_remove).base (), block_callback_data.end ());
				receive_details.set_source_block_callback_data (std::vector<nano::block_hash>{});
			}
		}

		nano::confirmation_height_unbounded::conf_height_details details{ preparation_data_a.account, preparation_data_a.current, block_height, num_blocks_confirmed, block_callback_data };
		rsnano::rsn_conf_height_unbounded_pending_writes_add (handle, details.handle);
	}

	if (!receive_details.is_null ())
	{
		// Check whether the previous block has been seen. If so, the rest of sends below have already been seen so don't count them
		auto receive_account = receive_details.get_account ();
		rsnano::ConfirmedIteratedPairsIteratorDto receive_account_it;
		rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_find (handle, receive_account.bytes.data (), &receive_account_it);
		if (!receive_account_it.is_end)
		{
			// Get current height
			auto current_height = receive_account_it.confirmed_height;
			rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_set_confirmed_height (handle, receive_account.bytes.data (), receive_details.get_height ());
			auto const orig_num_blocks_confirmed = receive_details.get_num_blocks_confirmed ();
			receive_details.set_num_blocks_confirmed (receive_details.get_height () - current_height);

			// Get the difference and remove the callbacks
			auto block_callbacks_to_remove = orig_num_blocks_confirmed - receive_details.get_num_blocks_confirmed ();
			auto tmp_blocks{ receive_details.get_block_callback_data () };
			tmp_blocks.erase (std::next (tmp_blocks.rbegin (), block_callbacks_to_remove).base (), tmp_blocks.end ());
			receive_details.set_block_callback_data (tmp_blocks);
			debug_assert (receive_details.get_block_callback_data ().size () == receive_details.get_num_blocks_confirmed ());
		}
		else
		{
			rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_insert (handle, receive_account.bytes.data (), receive_details.get_height (), receive_details.get_height ());
		}

		rsnano::rsn_conf_height_unbounded_pending_writes_add2 (handle, receive_details.handle);
	}
}

void nano::confirmation_height_unbounded::cement_blocks (nano::write_guard & scoped_write_guard_a)
{
	rsnano::rsn_conf_height_unbounded_cement_blocks (handle, scoped_write_guard_a.handle);
}

std::shared_ptr<nano::block> nano::confirmation_height_unbounded::get_block_and_sideband (nano::block_hash const & hash_a, nano::transaction const & transaction_a)
{
	auto block_handle{ rsnano::rsn_conf_height_unbounded_get_block_and_sideband (handle, hash_a.bytes.data (), transaction_a.get_rust_handle ()) };
	return nano::block_handle_to_block (block_handle);
}

bool nano::confirmation_height_unbounded::pending_empty () const
{
	return rsnano::rsn_conf_height_unbounded_pending_empty (handle);
}

void nano::confirmation_height_unbounded::clear_process_vars ()
{
	rsnano::rsn_conf_height_unbounded_clear_process_vars (handle);
}

bool nano::confirmation_height_unbounded::has_iterated_over_block (nano::block_hash const & hash_a) const
{
	return rsnano::rsn_conf_height_unbounded_has_iterated_over_block (handle, hash_a.bytes.data ());
}

void nano::confirmation_height_unbounded::stop ()
{
	stopped = true;
}

uint64_t nano::confirmation_height_unbounded::block_cache_size () const
{
	return rsnano::rsn_conf_height_unbounded_block_cache_size (handle);
}

nano::confirmation_height_unbounded::conf_height_details::conf_height_details (nano::account const & account_a, nano::block_hash const & hash_a, uint64_t height_a, uint64_t num_blocks_confirmed_a, std::vector<nano::block_hash> const & block_callback_data_a) :
	handle{ rsnano::rsn_conf_height_details_create (account_a.bytes.data (), hash_a.bytes.data (), height_a, num_blocks_confirmed_a) }
{
	for (auto b : block_callback_data_a)
	{
		add_block_callback_data (b);
	}
}

nano::confirmation_height_unbounded::conf_height_details::conf_height_details (nano::confirmation_height_unbounded::conf_height_details const & other_a) :
	handle{ rsnano::rsn_conf_height_details_clone (other_a.handle) }
{
}

nano::confirmation_height_unbounded::conf_height_details::~conf_height_details ()
{
	rsnano::rsn_conf_height_details_destroy (handle);
}

nano::confirmation_height_unbounded::conf_height_details & nano::confirmation_height_unbounded::conf_height_details::operator= (nano::confirmation_height_unbounded::conf_height_details const & other_a)
{
	rsnano::rsn_conf_height_details_destroy (handle);
	handle = rsnano::rsn_conf_height_details_clone (other_a.handle);
	return *this;
}

void nano::confirmation_height_unbounded::conf_height_details::add_block_callback_data (nano::block_hash const & hash)
{
	rsnano::rsn_conf_height_details_add_block_callback_data (handle, hash.bytes.data ());
}

nano::confirmation_height_unbounded::receive_source_pair::receive_source_pair (conf_height_details_shared_ptr const & receive_details_a, const block_hash & source_a) :
	handle{ rsnano::rsn_receive_source_pair_create (receive_details_a.handle, source_a.bytes.data ()) }
{
}

nano::confirmation_height_unbounded::receive_source_pair::receive_source_pair (rsnano::ReceiveSourcePairHandle * handle_a) :
	handle{ handle_a }
{
}

nano::confirmation_height_unbounded::receive_source_pair::receive_source_pair (nano::confirmation_height_unbounded::receive_source_pair const & other_a) :
	handle{ rsnano::rsn_receive_source_pair_clone (other_a.handle) }
{
}

nano::confirmation_height_unbounded::receive_source_pair::~receive_source_pair ()
{
	rsnano::rsn_receive_source_pair_destroy (handle);
}
nano::confirmation_height_unbounded::receive_source_pair & nano::confirmation_height_unbounded::receive_source_pair::operator= (receive_source_pair const & other_a)
{
	rsnano::rsn_receive_source_pair_destroy (handle);
	handle = rsnano::rsn_receive_source_pair_clone (other_a.handle);
	return *this;
}

nano::confirmation_height_unbounded::conf_height_details_shared_ptr nano::confirmation_height_unbounded::receive_source_pair::receive_details () const
{
	return nano::confirmation_height_unbounded::conf_height_details_shared_ptr (rsnano::rsn_receive_source_pair_receive_details (handle));
}
nano::block_hash nano::confirmation_height_unbounded::receive_source_pair::source_hash () const
{
	nano::block_hash hash;
	rsnano::rsn_receive_source_pair_source_hash (handle, hash.bytes.data ());
	return hash;
}

std::unique_ptr<nano::container_info_component> nano::collect_container_info (confirmation_height_unbounded & confirmation_height_unbounded, std::string const & name_a)
{
	auto composite = std::make_unique<container_info_composite> (name_a);
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "confirmed_iterated_pairs", rsnano::rsn_conf_height_unbounded_conf_iterated_pairs_len (confirmation_height_unbounded.handle), rsnano::rsn_conf_iterated_pair_size () }));
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "pending_writes", rsnano::rsn_conf_height_unbounded_pending_writes_len (confirmation_height_unbounded.handle), rsnano::rsn_conf_height_details_size () }));
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "implicit_receive_cemented_mapping", rsnano::rsn_conf_height_unbounded_implicit_receive_cemented_mapping_size (confirmation_height_unbounded.handle), rsnano::rsn_implicit_receive_cemented_mapping_value_size () }));
	composite->add_component (std::make_unique<container_info_leaf> (container_info{ "block_cache", confirmation_height_unbounded.block_cache_size (), rsnano::rsn_conf_height_unbounded_block_cache_element_size () }));
	return composite;
}

nano::confirmation_height_unbounded::receive_source_pair_vec::receive_source_pair_vec () :
	handle{ rsnano::rsn_receive_source_pair_vec_create () }
{
}
nano::confirmation_height_unbounded::receive_source_pair_vec::~receive_source_pair_vec ()
{
	rsnano::rsn_receive_source_pair_vec_destroy (handle);
}
bool nano::confirmation_height_unbounded::receive_source_pair_vec::empty () const
{
	return size () == 0;
}
size_t nano::confirmation_height_unbounded::receive_source_pair_vec::size () const
{
	return rsnano::rsn_receive_source_pair_vec_size (handle);
}

void nano::confirmation_height_unbounded::receive_source_pair_vec::push (nano::confirmation_height_unbounded::receive_source_pair const & pair)
{
	rsnano::rsn_receive_source_pair_vec_push (handle, pair.handle);
}

void nano::confirmation_height_unbounded::receive_source_pair_vec::pop ()
{
	rsnano::rsn_receive_source_pair_vec_pop (handle);
}

nano::confirmation_height_unbounded::receive_source_pair nano::confirmation_height_unbounded::receive_source_pair_vec::back () const
{
	return nano::confirmation_height_unbounded::receive_source_pair{ rsnano::rsn_receive_source_pair_vec_back (handle) };
}
