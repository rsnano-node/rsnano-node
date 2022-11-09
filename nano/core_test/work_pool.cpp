#include <nano/crypto_lib/random_pool.hpp>
#include <nano/lib/blocks.hpp>
#include <nano/lib/logger_mt.hpp>
#include <nano/lib/timer.hpp>
#include <nano/lib/work.hpp>
#include <nano/node/logging.hpp>
#include <nano/node/openclconfig.hpp>
#include <nano/node/openclwork.hpp>
#include <nano/secure/common.hpp>
#include <nano/secure/utility.hpp>

#include <gtest/gtest.h>

#include <future>

TEST (work, opencl)
{
	nano::logging logging;
	logging.init (nano::unique_path ());
	nano::logger_mt logger;
	bool error (false);
	nano::opencl_environment environment (error);
	ASSERT_TRUE (!error || !nano::opencl_loaded);
	if (!environment.platforms.empty () && !environment.platforms.begin ()->devices.empty ())
	{
		nano::opencl_config config (0, 0, 16 * 1024);
		auto opencl (nano::opencl_work::create (true, config, logger, nano::dev::network_params.work));
		if (opencl != nullptr)
		{
			// 0 threads, should add 1 for managing OpenCL
			nano::work_pool pool{ nano::dev::network_params.network, 0, std::chrono::nanoseconds (0), [&opencl] (nano::work_version const version_a, nano::root const & root_a, uint64_t difficulty_a, nano::work_ticket ticket_a) {
									 return opencl->generate_work (version_a, root_a, difficulty_a);
								 } };
			ASSERT_TRUE (pool.has_opencl ());
			nano::root root;
			uint64_t difficulty (0xff00000000000000);
			uint64_t difficulty_add (0x000f000000000000);
			for (auto i (0); i < 16; ++i)
			{
				nano::random_pool::generate_block (root.bytes.data (), root.bytes.size ());
				auto result (*pool.generate (nano::work_version::work_1, root, difficulty));
				ASSERT_GE (nano::dev::network_params.work.difficulty (nano::work_version::work_1, root, result), difficulty);
				difficulty += difficulty_add;
			}
		}
		else
		{
			std::cerr << "Error starting OpenCL test" << std::endl;
		}
	}
	else
	{
		std::cout << "Device with OpenCL support not found. Skipping OpenCL test" << std::endl;
	}
}

TEST (work, eco_pow)
{
	auto work_func = [] (std::promise<std::chrono::nanoseconds> & promise, std::chrono::nanoseconds interval) {
		nano::work_pool pool{ nano::dev::network_params.network, 1, interval };
		constexpr auto num_iterations = 5;

		nano::timer<std::chrono::nanoseconds> timer;
		timer.start ();
		for (int i = 0; i < num_iterations; ++i)
		{
			nano::root root (1);
			uint64_t difficulty1 (0xff00000000000000);
			uint64_t difficulty2 (0xfff0000000000000);
			uint64_t result_difficulty (0);
			do
			{
				auto work = *pool.generate (nano::work_version::work_1, root, difficulty1);
				result_difficulty = nano::dev::network_params.work.difficulty (nano::work_version::work_1, root, work);
			} while (result_difficulty > difficulty2);
			ASSERT_GT (result_difficulty, difficulty1);
		}

		promise.set_value_at_thread_exit (timer.stop ());
	};

	std::promise<std::chrono::nanoseconds> promise1;
	std::future<std::chrono::nanoseconds> future1 = promise1.get_future ();
	std::promise<std::chrono::nanoseconds> promise2;
	std::future<std::chrono::nanoseconds> future2 = promise2.get_future ();

	std::thread thread1 (work_func, std::ref (promise1), std::chrono::nanoseconds (0));
	std::thread thread2 (work_func, std::ref (promise2), std::chrono::milliseconds (10));

	thread1.join ();
	thread2.join ();

	// Confirm that the eco pow rate limiter is working.
	// It's possible under some unlucky circumstances that this fails to the random nature of valid work generation.
	ASSERT_LT (future1.get (), future2.get ());
}
