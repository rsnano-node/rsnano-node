#include "nano/lib/rsnano.hpp"

#include <nano/node/inactive_cache_status.hpp>

nano::inactive_cache_status::inactive_cache_status () :
	handle (rsnano::rsn_inactive_cache_status_create ())
{
}

bool nano::inactive_cache_status::get_bootstrap_started () const
{
	return rsnano::rsn_inactive_cache_status_bootstrap_started (handle);
}

bool nano::inactive_cache_status::get_election_started () const
{
	return rsnano::rsn_inactive_cache_status_election_started (handle);
}

bool nano::inactive_cache_status::get_confirmed() const
{
	return rsnano::rsn_inactive_cache_status_confirmed (handle);
}

nano::uint128_t nano::inactive_cache_status::get_tally () const
{
	nano::uint128_t tally;
	uint8_t * rsn_tally;
	rsnano::rsn_inactive_cache_status_tally (handle, rsn_tally);
	boost::multiprecision::export_bits (tally, rsn_tally, 8, false);
	//boost::multiprecision::import_bits (tally, std::begin (rsn_tally), std::end (rsn_tally));
	return tally;
}

bool nano::inactive_cache_status::operator!= (inactive_cache_status const other) const
{
	uint8_t * rsn_tally;
	nano::uint128_t other_tally;
	rsnano::rsn_inactive_cache_status_tally (other.handle, rsn_tally);
	boost::multiprecision::export_bits (other_tally, rsn_tally, 8, false);

	return rsnano::rsn_inactive_cache_status_bootstrap_started (handle) != rsnano::rsn_inactive_cache_status_bootstrap_started (other.handle)
	|| rsnano::rsn_inactive_cache_status_election_started (handle) != rsnano::rsn_inactive_cache_status_election_started (other.handle)
	|| rsnano::rsn_inactive_cache_status_confirmed (handle) != rsnano::rsn_inactive_cache_status_confirmed (other.handle)
	|| get_tally() != other_tally;
}

std::string nano::inactive_cache_status::to_string () const
{
	std::stringstream ss;
	ss << "bootstrap_started=" << get_bootstrap_started ();
	ss << ", election_started=" << get_election_started ();
	ss << ", confirmed=" << get_confirmed ();
	ss << ", tally=" << nano::uint128_union (get_tally ()).to_string ();
	return ss.str ();
}
