#include <nano/lib/thread_roles.hpp>
#include <nano/lib/timer.hpp>
#include <nano/store/component.hpp>

std::optional<nano::account_info> nano::account_store::get (const nano::transaction & transaction, const nano::account & account)
{
	nano::account_info info;
	bool error = get (transaction, account, info);
	if (!error)
	{
		return info;
	}
	else
	{
		return std::nullopt;
	}
}

std::optional<nano::confirmation_height_info> nano::confirmation_height_store::get (const nano::transaction & transaction, const nano::account & account)
{
	nano::confirmation_height_info info;
	bool error = get (transaction, account, info);
	if (!error)
	{
		return info;
	}
	else
	{
		return std::nullopt;
	}
}