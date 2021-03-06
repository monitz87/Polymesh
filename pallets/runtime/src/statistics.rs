use polymesh_primitives::Ticker;
use polymesh_runtime_common::balances::Trait as BalancesTrait;

use frame_support::{decl_module, decl_storage};

type Counter = u64;

pub trait Trait: BalancesTrait {}

decl_storage! {
    trait Store for Module<T: Trait> as statistics {
        pub InvestorCountPerAsset get(fn investor_count_per_asset): map Ticker => Counter ;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    }
}

impl<T: Trait> Module<T> {
    /// It updates our statistics after transfer execution.
    /// The following counters could be updated:
    ///     - *Investor count per asset*.
    ///
    pub fn update_transfer_stats(
        ticker: &Ticker,
        updated_from_balance: Option<T::Balance>,
        updated_to_balance: Option<T::Balance>,
        amount: T::Balance,
    ) {
        // 1. Investor count per asset.
        if amount != 0u128.into() {
            let counter = Self::investor_count_per_asset(ticker);
            let mut new_counter = counter;

            if let Some(from_balance) = updated_from_balance {
                if from_balance == 0u128.into() {
                    new_counter = new_counter.checked_sub(1).unwrap_or(new_counter);
                }
            }

            if let Some(to_balance) = updated_to_balance {
                if to_balance == amount {
                    new_counter = new_counter.checked_add(1).unwrap_or(new_counter);
                }
            }

            // Only updates extrinsics if counter has been changed.
            if new_counter != counter {
                <InvestorCountPerAsset>::insert(ticker, new_counter)
            }
        }
    }
}
