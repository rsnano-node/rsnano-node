#pragma once

#include <nano/store/component.hpp>

namespace nano
{
namespace lmdb
{
	class block_store : public nano::block_store
	{
		rsnano::LmdbBlockStoreHandle * handle;

	public:
		explicit block_store (rsnano::LmdbBlockStoreHandle * handle_a);
		block_store (block_store const &) = delete;
		block_store (block_store &&) = delete;
		~block_store () override;
		void put (nano::write_transaction const & transaction_a, nano::block_hash const & hash_a, nano::block const & block_a) override;
		void raw_put (nano::write_transaction const & transaction_a, std::vector<uint8_t> const & data, nano::block_hash const & hash_a) override;
		nano::block_hash successor (nano::transaction const & transaction_a, nano::block_hash const & hash_a) const override;
		void successor_clear (nano::write_transaction const & transaction_a, nano::block_hash const & hash_a) override;
		std::shared_ptr<nano::block> get (nano::transaction const & transaction_a, nano::block_hash const & hash_a) const override;
		std::shared_ptr<nano::block> random (nano::transaction const & transaction_a) override;
		void del (nano::write_transaction const & transaction_a, nano::block_hash const & hash_a) override;
		bool exists (nano::transaction const & transaction_a, nano::block_hash const & hash_a) override;
		uint64_t count (nano::transaction const & transaction_a) override;
		nano::store_iterator<nano::block_hash, nano::block_w_sideband> begin (nano::transaction const & transaction_a) const override;
		nano::store_iterator<nano::block_hash, nano::block_w_sideband> begin (nano::transaction const & transaction_a, nano::block_hash const & hash_a) const override;
		nano::store_iterator<nano::block_hash, nano::block_w_sideband> end () const override;
		void for_each_par (std::function<void (nano::read_transaction const &, nano::store_iterator<nano::block_hash, block_w_sideband>, nano::store_iterator<nano::block_hash, block_w_sideband>)> const & action_a) const override;
	};
}
}