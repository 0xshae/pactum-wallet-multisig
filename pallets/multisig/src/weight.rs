#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_multisig.
pub trait WeightInfo {
    fn create_multisig() -> Weight;
    fn submit_proposal() -> Weight;
    fn confirm_proposal() -> Weight;
    fn execute_proposal() -> Weight;
}

/// A dummy implementation for testing purposes.
impl WeightInfo for () {
    fn create_multisig() -> Weight {
        Weight::from_parts(10_000, 0)
            .saturating_add(Weight::from_parts(100_000_000, 0))
    }
    
    fn submit_proposal() -> Weight {
        Weight::from_parts(20_000, 0)
            .saturating_add(Weight::from_parts(150_000_000, 0))
    }

    fn confirm_proposal() -> Weight {
        Weight::from_parts(20_000, 0)
            .saturating_add(Weight::from_parts(150_000_000, 0))
    }
    fn execute_proposal() -> Weight {
        Weight::from_parts(20_000, 0)
            .saturating_add(Weight::from_parts(150_000_000, 0))
    }
}
