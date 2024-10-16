#include <nano/lib/blocks.hpp>
#include <nano/lib/logging.hpp>
#include <nano/lib/rsnano.hpp>
#include <nano/lib/rsnanoutils.hpp>
#include <nano/lib/stats.hpp>
#include <nano/lib/utility.hpp>
#include <nano/secure/common.hpp>
#include <nano/secure/ledger.hpp>
#include <nano/store/account.hpp>
#include <nano/store/block.hpp>
#include <nano/store/component.hpp>
#include <nano/store/confirmation_height.hpp>
#include <nano/store/final.hpp>
#include <nano/store/online_weight.hpp>
#include <nano/store/peer.hpp>
#include <nano/store/pending.hpp>
#include <nano/store/pruned.hpp>
#include <nano/store/version.hpp>

#include <boost/multiprecision/cpp_int.hpp>

#include <optional>

nano::ledger::ledger (rsnano::LedgerHandle * handle, nano::store::component & store_a, nano::ledger_constants & constants) :
	handle{ handle },
	constants{ constants },
	store{ store_a }
{
}

nano::ledger::~ledger ()
{
	rsnano::rsn_ledger_destroy (handle);
}

rsnano::LedgerHandle * nano::ledger::get_handle () const
{
	return handle;
}

nano::ledger_set_any nano::ledger::any () const
{
	return { rsnano::rsn_ledger_any (handle) };
}

nano::ledger_set_confirmed nano::ledger::confirmed () const
{
	return { rsnano::rsn_ledger_confirmed (handle) };
}

nano::store::write_guard nano::ledger::wait ()
{
	auto guard_handle = rsnano::rsn_ledger_wait (handle);
	return nano::store::write_guard (guard_handle);
}

bool nano::ledger::queue_contains (nano::store::writer writer)
{
	return rsnano::rsn_ledger_queue_contains (handle, static_cast<uint8_t> (writer));
}

nano::uint128_t nano::ledger::account_receivable (store::transaction const & transaction_a, nano::account const & account_a, bool only_confirmed_a)
{
	nano::amount result;
	rsnano::rsn_ledger_account_receivable (handle, transaction_a.get_rust_handle (), account_a.bytes.data (), only_confirmed_a, result.bytes.data ());
	return result.number ();
}

std::deque<std::shared_ptr<nano::block>> nano::ledger::confirm (nano::store::write_transaction const & transaction, nano::block_hash const & hash)
{
	rsnano::BlockArrayDto dto;
	rsnano::rsn_ledger_confirm (handle, transaction.get_rust_handle (), hash.bytes.data (), &dto);
	std::deque<std::shared_ptr<nano::block>> blocks;
	rsnano::read_block_deque (dto, blocks);
	return blocks;
}

nano::block_status nano::ledger::process (store::write_transaction const & transaction_a, std::shared_ptr<nano::block> block_a)
{
	rsnano::ProcessReturnDto result_dto;
	rsnano::rsn_ledger_process (handle, transaction_a.get_rust_handle (), block_a->get_handle (), &result_dto);
	return static_cast<nano::block_status> (result_dto.code);
}

nano::block_hash nano::ledger::representative (store::transaction const & transaction_a, nano::block_hash const & hash_a)
{
	nano::block_hash result;
	rsnano::rsn_ledger_representative (handle, transaction_a.get_rust_handle (), hash_a.bytes.data (), result.bytes.data ());
	return result;
}

std::string nano::ledger::block_text (char const * hash_a)
{
	return block_text (nano::block_hash (hash_a));
}

std::string nano::ledger::block_text (nano::block_hash const & hash_a)
{
	rsnano::StringDto dto;
	rsnano::rsn_ledger_block_text (handle, hash_a.bytes.data (), &dto);
	return rsnano::convert_dto_to_string (dto);
}

std::pair<nano::block_hash, nano::block_hash> nano::ledger::hash_root_random (store::transaction const & transaction_a) const
{
	nano::block_hash hash;
	nano::block_hash root;
	rsnano::rsn_ledger_hash_root_random (handle, transaction_a.get_rust_handle (), hash.bytes.data (), root.bytes.data ());
	return std::make_pair (hash, root);
}

// Vote weight of an account
nano::uint128_t nano::ledger::weight (nano::account const & account_a) const
{
	nano::amount result;
	rsnano::rsn_ledger_weight (handle, account_a.bytes.data (), result.bytes.data ());
	return result.number ();
}

nano::uint128_t nano::ledger::weight_exact (store::transaction const & txn_a, nano::account const & representative_a) const
{
	nano::amount result;
	rsnano::rsn_ledger_weight_exact (handle, txn_a.get_rust_handle (), representative_a.bytes.data (), result.bytes.data ());
	return result.number ();
}

// Rollback blocks until `block_a' doesn't exist or it tries to penetrate the confirmation height
bool nano::ledger::rollback (store::write_transaction const & transaction_a, nano::block_hash const & block_a, std::vector<std::shared_ptr<nano::block>> & list_a)
{
	rsnano::BlockArrayDto list_dto;
	auto error = rsnano::rsn_ledger_rollback (handle, transaction_a.get_rust_handle (), block_a.bytes.data (), &list_dto);
	rsnano::read_block_array_dto (list_dto, list_a);
	return error;
}

bool nano::ledger::rollback (store::write_transaction const & transaction_a, nano::block_hash const & block_a)
{
	std::vector<std::shared_ptr<nano::block>> rollback_list;
	return rollback (transaction_a, block_a, rollback_list);
}

// Return latest root for account, account number if there are no blocks for this account.
nano::root nano::ledger::latest_root (store::transaction const & transaction_a, nano::account const & account_a)
{
	nano::root latest_l;
	rsnano::rsn_ledger_latest_root (handle, transaction_a.get_rust_handle (), account_a.bytes.data (), latest_l.bytes.data ());
	return latest_l;
}

bool nano::ledger::dependents_confirmed (store::transaction const & transaction_a, nano::block const & block_a) const
{
	return rsnano::rsn_ledger_dependents_confirmed (handle, transaction_a.get_rust_handle (), block_a.get_handle ());
}

bool nano::ledger::is_epoch_link (nano::link const & link_a) const
{
	return rsnano::rsn_ledger_is_epoch_link (handle, link_a.bytes.data ());
}

/** Given the block hash of a send block, find the associated receive block that receives that send.
 *  The send block hash is not checked in any way, it is assumed to be correct.
 * @return Return the receive block on success and null on failure
 */
std::shared_ptr<nano::block> nano::ledger::find_receive_block_by_send_hash (store::transaction const & transaction, nano::account const & destination, nano::block_hash const & send_block_hash)
{
	auto block_handle = rsnano::rsn_ledger_find_receive_block_by_send_hash (handle, transaction.get_rust_handle (), destination.bytes.data (), send_block_hash.bytes.data ());
	return nano::block_handle_to_block (block_handle);
}

nano::account nano::ledger::epoch_signer (nano::link const & link_a) const
{
	nano::account signer;
	rsnano::rsn_ledger_epoch_signer (handle, link_a.bytes.data (), signer.bytes.data ());
	return signer;
}

nano::link nano::ledger::epoch_link (nano::epoch epoch_a) const
{
	nano::link link;
	rsnano::rsn_ledger_epoch_link (handle, static_cast<uint8_t> (epoch_a), link.bytes.data ());
	return link;
}

void nano::ledger::update_account (store::write_transaction const & transaction_a, nano::account const & account_a, nano::account_info const & old_a, nano::account_info const & new_a)
{
	rsnano::rsn_ledger_update_account (handle, transaction_a.get_rust_handle (), account_a.bytes.data (), old_a.handle, new_a.handle);
}

uint64_t nano::ledger::pruning_action (store::write_transaction & transaction_a, nano::block_hash const & hash_a, uint64_t const batch_size_a)
{
	return rsnano::rsn_ledger_pruning_action (handle, transaction_a.get_rust_handle (), hash_a.bytes.data (), batch_size_a);
}

bool nano::ledger::pruning_enabled () const
{
	return rsnano::rsn_ledger_pruning_enabled (handle);
}

nano::epoch nano::ledger::version (nano::block const & block)
{
	if (block.type () == nano::block_type::state)
	{
		return block.sideband ().details ().epoch ();
	}

	return nano::epoch::epoch_0;
}

nano::epoch nano::ledger::version (store::transaction const & transaction, nano::block_hash const & hash) const
{
	auto epoch = rsnano::rsn_ledger_version (handle, transaction.get_rust_handle (), hash.bytes.data ());
	return static_cast<nano::epoch> (epoch);
}

uint64_t nano::ledger::cemented_count () const
{
	return rsnano::rsn_ledger_cemented_count (handle);
}

uint64_t nano::ledger::block_count () const
{
	return rsnano::rsn_ledger_block_count (handle);
}

uint64_t nano::ledger::account_count () const
{
	return rsnano::rsn_ledger_account_count (handle);
}

uint64_t nano::ledger::pruned_count () const
{
	return rsnano::rsn_ledger_pruned_count (handle);
}

nano::ledger_set_any::ledger_set_any (rsnano::LedgerSetAnyHandle * handle) :
	handle{ handle }
{
}

nano::ledger_set_any::~ledger_set_any ()
{
	rsnano::rsn_ledger_set_any_destroy (handle);
}

std::optional<nano::account_info> nano::ledger_set_any::account_get (store::transaction const & transaction, nano::account const & account) const
{
	auto info_handle = rsnano::rsn_ledger_set_any_get_account (handle, transaction.get_rust_handle (), account.bytes.data ());
	if (info_handle != nullptr)
	{
		return { info_handle };
	}
	else
	{
		return std::nullopt;
	}
}

bool nano::ledger_set_any::block_exists_or_pruned (store::transaction const & transaction, nano::block_hash const & hash) const
{
	return rsnano::rsn_ledger_set_any_block_exists_or_pruned (handle, transaction.get_rust_handle (), hash.bytes.data ());
}

bool nano::ledger_set_any::block_exists (store::transaction const & transaction, nano::block_hash const & hash) const
{
	return rsnano::rsn_ledger_set_any_block_exists (handle, transaction.get_rust_handle (), hash.bytes.data ());
}

std::shared_ptr<nano::block> nano::ledger_set_any::block_get (store::transaction const & transaction, nano::block_hash const & hash) const
{
	auto block_handle = rsnano::rsn_ledger_set_any_block_get (handle, transaction.get_rust_handle (), hash.bytes.data ());
	return nano::block_handle_to_block (block_handle);
}

std::optional<nano::amount> nano::ledger_set_any::block_balance (store::transaction const & transaction, nano::block_hash const & hash) const
{
	nano::amount balance;
	if (rsnano::rsn_ledger_set_any_block_balance (handle, transaction.get_rust_handle (), hash.bytes.data (), balance.bytes.data ()))
	{
		return balance;
	}
	else
	{
		return std::nullopt;
	}
}

nano::block_hash nano::ledger_set_any::account_head (store::transaction const & transaction, nano::account const & account) const
{
	nano::block_hash head;
	if (rsnano::rsn_ledger_set_any_account_head (handle, transaction.get_rust_handle (), account.bytes.data (), head.bytes.data ()))
	{
		return head;
	}
	else
	{
		return { 0 };
	}
}

std::optional<nano::account> nano::ledger_set_any::block_account (store::transaction const & transaction, nano::block_hash const & hash) const
{
	nano::account account;
	if (rsnano::rsn_ledger_set_any_block_account (handle, transaction.get_rust_handle (), hash.bytes.data (), account.bytes.data ()))
	{
		return account;
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::amount> nano::ledger_set_any::block_amount (store::transaction const & transaction, nano::block_hash const & hash) const
{
	nano::amount amount;
	if (rsnano::rsn_ledger_set_any_block_amount (handle, transaction.get_rust_handle (), hash.bytes.data (), amount.bytes.data ()))
	{
		return amount;
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::amount> nano::ledger_set_any::account_balance (store::transaction const & transaction, nano::account const & account) const
{
	nano::amount balance;
	if (rsnano::rsn_ledger_set_any_account_balance (handle, transaction.get_rust_handle (), account.bytes.data (), balance.bytes.data ()))
	{
		return balance;
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::pending_info> nano::ledger_set_any::pending_get (store::transaction const & transaction, nano::pending_key const & key) const
{
	rsnano::PendingInfoDto info;
	if (rsnano::rsn_ledger_set_any_pending_get (handle, transaction.get_rust_handle (), key.account.bytes.data (), key.hash.bytes.data (), &info))
	{
		return { info };
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::block_hash> nano::ledger_set_any::block_successor (store::transaction const & transaction, nano::block_hash const & hash) const
{
	nano::block_hash successor;
	if (rsnano::rsn_ledger_set_any_block_successor (handle, transaction.get_rust_handle (), hash.bytes.data (), successor.bytes.data ()))
	{
		return successor;
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::block_hash> nano::ledger_set_any::block_successor (store::transaction const & transaction, nano::qualified_root const & root) const
{
	nano::block_hash successor;
	if (rsnano::rsn_ledger_set_any_block_successor_root (handle, transaction.get_rust_handle (), root.bytes.data (), successor.bytes.data ()))
	{
		return successor;
	}
	else
	{
		return std::nullopt;
	}
}

nano::receivable_iterator nano::ledger_set_any::receivable_upper_bound (store::transaction const & transaction, nano::account const & account, nano::block_hash const & hash) const
{
	return { rsnano::rsn_ledger_set_any_account_receivable_upper_bound (handle, transaction.get_rust_handle (), account.bytes.data (), hash.bytes.data ()) };
}

nano::receivable_iterator nano::ledger_set_any::receivable_upper_bound (store::transaction const & transaction, nano::account const & account) const
{
	return { rsnano::rsn_ledger_set_any_receivable_upper_bound (handle, transaction.get_rust_handle (), account.bytes.data ()) };
}

nano::ledger_set_confirmed::ledger_set_confirmed (rsnano::LedgerSetConfirmedHandle * handle) :
	handle{ handle }
{
}

nano::ledger_set_confirmed::~ledger_set_confirmed ()
{
	rsnano::rsn_ledger_set_confirmed_destroy (handle);
}

bool nano::ledger_set_confirmed::block_exists_or_pruned (store::transaction const & transaction, nano::block_hash const & hash) const
{
	return rsnano::rsn_ledger_set_confirmed_block_exists_or_pruned (handle, transaction.get_rust_handle (), hash.bytes.data ());
}

bool nano::ledger_set_confirmed::block_exists (store::transaction const & transaction, nano::block_hash const & hash) const
{
	return rsnano::rsn_ledger_set_confirmed_block_exists (handle, transaction.get_rust_handle (), hash.bytes.data ());
}

std::optional<nano::amount> nano::ledger_set_confirmed::account_balance (store::transaction const & transaction, nano::account const & account) const
{
	nano::amount balance;
	if (rsnano::rsn_ledger_set_confirmed_account_balance (handle, transaction.get_rust_handle (), account.bytes.data (), balance.bytes.data ()))
	{
		return balance;
	}
	else
	{
		return std::nullopt;
	}
}
