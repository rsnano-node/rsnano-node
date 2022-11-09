#include <nano/node/common.hpp>
#include <nano/node/voting.hpp>
#include <nano/test_common/system.hpp>
#include <nano/test_common/testutil.hpp>

#include <gtest/gtest.h>

using namespace std::chrono_literals;

TEST (vote_generator, cache)
{
	nano::test::system system (1);
	auto & node (*system.nodes[0]);
	auto epoch1 = system.upgrade_genesis_epoch (node, nano::epoch::epoch_1);
	system.wallet (0)->insert_adhoc (nano::dev::genesis_key.prv);
	node.generator.add (epoch1->root (), epoch1->hash ());
	ASSERT_TIMELY (1s, !node.history.votes (epoch1->root (), epoch1->hash ()).empty ());
	auto votes (node.history.votes (epoch1->root (), epoch1->hash ()));
	ASSERT_FALSE (votes.empty ());
	auto hashes{ votes[0]->hashes () };
	ASSERT_TRUE (std::any_of (hashes.begin (), hashes.end (), [hash = epoch1->hash ()] (nano::block_hash const & hash_a) { return hash_a == hash; }));
}

TEST (vote_generator, multiple_representatives)
{
	nano::test::system system (1);
	auto & node (*system.nodes[0]);
	nano::keypair key1, key2, key3;
	auto & wallet (*system.wallet (0));
	wallet.insert_adhoc (nano::dev::genesis_key.prv);
	wallet.insert_adhoc (key1.prv);
	wallet.insert_adhoc (key2.prv);
	wallet.insert_adhoc (key3.prv);
	auto const amount = 100 * nano::Gxrb_ratio;
	wallet.send_sync (nano::dev::genesis_key.pub, key1.pub, amount);
	wallet.send_sync (nano::dev::genesis_key.pub, key2.pub, amount);
	wallet.send_sync (nano::dev::genesis_key.pub, key3.pub, amount);
	ASSERT_TIMELY (3s, node.balance (key1.pub) == amount && node.balance (key2.pub) == amount && node.balance (key3.pub) == amount);
	wallet.change_sync (key1.pub, key1.pub);
	wallet.change_sync (key2.pub, key2.pub);
	wallet.change_sync (key3.pub, key3.pub);
	ASSERT_TRUE (node.weight (key1.pub) == amount && node.weight (key2.pub) == amount && node.weight (key3.pub) == amount);
	node.wallets.compute_reps ();
	ASSERT_EQ (4, node.wallets.reps ().voting);
	auto hash = wallet.send_sync (nano::dev::genesis_key.pub, nano::dev::genesis_key.pub, 1);
	auto send = node.block (hash);
	ASSERT_NE (nullptr, send);
	ASSERT_TIMELY (5s, node.history.votes (send->root (), send->hash ()).size () == 4);
	auto votes (node.history.votes (send->root (), send->hash ()));
	for (auto const & account : { key1.pub, key2.pub, key3.pub, nano::dev::genesis_key.pub })
	{
		auto existing (std::find_if (votes.begin (), votes.end (), [&account] (std::shared_ptr<nano::vote> const & vote_a) -> bool {
			return vote_a->account () == account;
		}));
		ASSERT_NE (votes.end (), existing);
	}
}

TEST (vote_generator, session)
{
	nano::test::system system (1);
	auto node (system.nodes[0]);
	system.wallet (0)->insert_adhoc (nano::dev::genesis_key.prv);
	nano::vote_generator_session generator_session (node->generator);
	boost::thread thread ([node, &generator_session] () {
		nano::thread_role::set (nano::thread_role::name::request_loop);
		generator_session.add (nano::dev::genesis->account (), nano::dev::genesis->hash ());
		ASSERT_EQ (0, node->stats->count (nano::stat::type::vote, nano::stat::detail::vote_indeterminate));
		generator_session.flush ();
	});
	thread.join ();
	ASSERT_TIMELY (2s, 1 == node->stats->count (nano::stat::type::vote, nano::stat::detail::vote_indeterminate));
}

TEST (vote_spacing, vote_generator)
{
	nano::node_config config;
	config.frontiers_confirmation = nano::frontiers_confirmation_mode::disabled;
	config.active_elections_hinted_limit_percentage = 0; // Disable election hinting
	nano::test::system system;
	nano::node_flags node_flags;
	node_flags.set_disable_search_pending (true);
	auto & node = *system.add_node (config, node_flags);
	auto & wallet = *system.wallet (0);
	wallet.insert_adhoc (nano::dev::genesis_key.prv);
	nano::state_block_builder builder;
	auto send1 = builder.make_block ()
				 .account (nano::dev::genesis_key.pub)
				 .previous (nano::dev::genesis->hash ())
				 .representative (nano::dev::genesis_key.pub)
				 .balance (nano::dev::constants.genesis_amount - nano::Gxrb_ratio)
				 .link (nano::dev::genesis_key.pub)
				 .sign (nano::dev::genesis_key.prv, nano::dev::genesis_key.pub)
				 .work (*system.work.generate (nano::dev::genesis->hash ()))
				 .build_shared ();
	auto send2 = builder.make_block ()
				 .account (nano::dev::genesis_key.pub)
				 .previous (nano::dev::genesis->hash ())
				 .representative (nano::dev::genesis_key.pub)
				 .balance (nano::dev::constants.genesis_amount - nano::Gxrb_ratio - 1)
				 .link (nano::dev::genesis_key.pub)
				 .sign (nano::dev::genesis_key.prv, nano::dev::genesis_key.pub)
				 .work (*system.work.generate (nano::dev::genesis->hash ()))
				 .build_shared ();
	ASSERT_EQ (nano::process_result::progress, node.ledger.process (*node.store.tx_begin_write (), *send1).code);
	ASSERT_EQ (0, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts));
	node.generator.add (nano::dev::genesis->hash (), send1->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts) == 1);
	ASSERT_FALSE (node.ledger.rollback (*node.store.tx_begin_write (), send1->hash ()));
	ASSERT_EQ (nano::process_result::progress, node.ledger.process (*node.store.tx_begin_write (), *send2).code);
	node.generator.add (nano::dev::genesis->hash (), send2->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_spacing) == 1);
	ASSERT_EQ (1, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts));
	std::this_thread::sleep_for (config.network_params.voting.delay);
	node.generator.add (nano::dev::genesis->hash (), send2->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts) == 2);
}

TEST (vote_spacing, rapid)
{
	nano::node_config config;
	config.frontiers_confirmation = nano::frontiers_confirmation_mode::disabled;
	config.active_elections_hinted_limit_percentage = 0; // Disable election hinting
	nano::test::system system;
	nano::node_flags node_flags;
	node_flags.set_disable_search_pending (true);
	auto & node = *system.add_node (config, node_flags);
	auto & wallet = *system.wallet (0);
	wallet.insert_adhoc (nano::dev::genesis_key.prv);
	nano::state_block_builder builder;
	auto send1 = builder.make_block ()
				 .account (nano::dev::genesis_key.pub)
				 .previous (nano::dev::genesis->hash ())
				 .representative (nano::dev::genesis_key.pub)
				 .balance (nano::dev::constants.genesis_amount - nano::Gxrb_ratio)
				 .link (nano::dev::genesis_key.pub)
				 .sign (nano::dev::genesis_key.prv, nano::dev::genesis_key.pub)
				 .work (*system.work.generate (nano::dev::genesis->hash ()))
				 .build_shared ();
	auto send2 = builder.make_block ()
				 .account (nano::dev::genesis_key.pub)
				 .previous (nano::dev::genesis->hash ())
				 .representative (nano::dev::genesis_key.pub)
				 .balance (nano::dev::constants.genesis_amount - nano::Gxrb_ratio - 1)
				 .link (nano::dev::genesis_key.pub)
				 .sign (nano::dev::genesis_key.prv, nano::dev::genesis_key.pub)
				 .work (*system.work.generate (nano::dev::genesis->hash ()))
				 .build_shared ();
	ASSERT_EQ (nano::process_result::progress, node.ledger.process (*node.store.tx_begin_write (), *send1).code);
	node.generator.add (nano::dev::genesis->hash (), send1->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts) == 1);
	ASSERT_FALSE (node.ledger.rollback (*node.store.tx_begin_write (), send1->hash ()));
	ASSERT_EQ (nano::process_result::progress, node.ledger.process (*node.store.tx_begin_write (), *send2).code);
	node.generator.add (nano::dev::genesis->hash (), send2->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_spacing) == 1);
	ASSERT_TIMELY (3s, 1 == node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts));
	std::this_thread::sleep_for (config.network_params.voting.delay);
	node.generator.add (nano::dev::genesis->hash (), send2->hash ());
	ASSERT_TIMELY (3s, node.stats->count (nano::stat::type::vote_generator, nano::stat::detail::generator_broadcasts) == 2);
}
