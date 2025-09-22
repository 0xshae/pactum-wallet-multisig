//! Benchmarking setup for pallet-multisig
#![cfg(feature = "runtime-benchmarks")]
use super::*;

#[allow(unused)]
use crate::Pallet as Multisig;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_std::prelude::*;
use codec::Encode;

// Helper to create a user account
fn create_user<T: Config>(name: &'static str, index: u32) -> T::AccountId {
	let user = frame_benchmarking::account(name, index, 0);
	user
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark for the `destroy_multisig` extrinsic, which is called via `execute_proposal`.
	/// This is the most complex extrinsic because `clear_prefix` depends on the number of
	/// proposals, `p`, that need to be deleted. We simulate this by creating `p` proposals
	/// before timing the destruction.
	#[benchmark(p = 1 .. 100)]
	fn destroy_multisig(p: u32) {
		let caller: T::AccountId = whitelisted_caller();
		let owners = vec![caller.clone(), create_user::<T>("owner", 1)];
		let threshold = 2;
		assert_ok!(Multisig::<T>::create_multisig(RawOrigin::Signed(caller.clone()).into(), owners.clone(), threshold));
		let multisig_id = Multisig::<T>::next_multisig_id() - 1;

		// Setup: Create `p` dummy proposals to fill up storage, simulating the worst case.
		for i in 0..p {
			let call: <T as Config>::RuntimeCall = frame_system::Call::remark { remark: i.encode() }.into();
			assert_ok!(Multisig::<T>::submit_proposal(RawOrigin::Signed(owners[0].clone()).into(), multisig_id, Box::new(call)));
		}

		// Now create the actual proposal to destroy the multisig.
		let destroy_call: <T as Config>::RuntimeCall = crate::Call::destroy_multisig { multisig_id }.into();
		assert_ok!(Multisig::<T>::submit_proposal(RawOrigin::Signed(owners[0].clone()).into(), multisig_id, Box::new(destroy_call.clone())));
		let proposal_index = p;
		assert_ok!(Multisig::<T>::confirm_proposal(RawOrigin::Signed(owners[1].clone()).into(), multisig_id, proposal_index));

		// The benchmarked action is the final execution of the destruction proposal.
		// We are benchmarking `execute_proposal` here because `destroy_multisig`
		// can only be called by the multisig's sovereign account. This setup correctly
		// measures the weight of the entire self-governed destruction process.
		#[extrinsic_call]
		execute_proposal(RawOrigin::Signed(caller), multisig_id, proposal_index, Box::new(destroy_call));

		// Verify that the multisig no longer exists.
		assert!(!<Multisigs<T>>::contains_key(multisig_id));
	}

	impl_benchmark_test_suite!(Multisig, crate::mock::new_test_ext(), crate::mock::Test);
}