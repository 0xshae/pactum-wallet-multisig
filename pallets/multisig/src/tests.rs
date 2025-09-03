use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_io::hashing::blake2_256;
use frame_system::Config;
use codec::Encode;

mod create_multisig {
	use super::*;

	#[test]
	fn it_creates_a_multisig_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let owners = vec![1, 2, 3];
			let threshold = 2;
			let creator = 1;

			assert_ok!(Multisig::create_multisig(
				RuntimeOrigin::signed(creator),
				owners.clone(),
				threshold
			));

			let multisig_id = 0;
			let multisig = Multisig::multisigs(multisig_id).unwrap();
			assert_eq!(multisig.owners.to_vec(), owners);
			assert_eq!(multisig.threshold, threshold);

			assert_eq!(Multisig::next_multisig_id(), 1);

			let multisig_account = Multisig::multi_account_id(multisig_id);
			System::assert_last_event(
				Event::MultisigCreated { creator, multisig_id, multisig_account }.into(),
			);
		});
	}

	#[test]
	fn fails_if_threshold_is_zero() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), vec![1, 2, 3], 0),
				Error::<Test>::InvalidThreshold
			);
		});
	}

	#[test]
	fn fails_if_threshold_is_greater_than_owners() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), vec![1, 2, 3], 4),
				Error::<Test>::InvalidThreshold
			);
		});
	}

	#[test]
	fn fails_if_too_many_owners() {
		new_test_ext().execute_with(|| {
			let owners = (1..=11).collect::<Vec<u64>>();
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), owners, 2),
				Error::<Test>::TooManyOwners
			);
		});
	}
}

mod submit_proposal {
	use super::*;

	// Helper function to create a multisig and return its ID.
	fn create_test_multisig() -> u32 {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
		// Return the ID of the created multisig (which is 0)
		0
	}

	#[test]
	fn it_submits_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let multisig_id = create_test_multisig();
			let proposer = 1; // An owner of the multisig

			// The call we want to propose
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![0; 10] }.into();
			let call_hash = blake2_256(&call.encode());

			// Dispatch the extrinsic
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(proposer),
				multisig_id,
				Box::new(call)
			));

			// Verify storage state
			let proposal_index = 0;

			// Check that the proposal was stored correctly
			let proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			assert_eq!(proposal.call_hash, call_hash);
			assert!(!proposal.executed);

			// Check that the proposer's approval was automatically recorded
			let expected_approvals: BoundedVec<u64, <Test as crate::Config>::MaxOwners> = vec![proposer].try_into().unwrap();			assert_eq!(Multisig::approvals(multisig_id, proposal_index), expected_approvals);


			// Check that the proposal index counter was incremented
			assert_eq!(Multisig::next_proposal_index(multisig_id), 1);

			// Verify that the correct event was emitted
			System::assert_last_event(
				Event::ProposalSubmitted { multisig_id, proposal_index, call_hash }.into(),
			);
		});
	}

	#[test]
	fn fails_if_multisig_not_found() {
		new_test_ext().execute_with(|| {
			let non_existent_multisig_id = 99;
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

			assert_noop!(
				Multisig::submit_proposal(
					RuntimeOrigin::signed(1),
					non_existent_multisig_id,
					Box::new(call)
				),
				Error::<Test>::MultisigNotFound
			);
		});
	}

	#[test]
	fn fails_if_not_an_owner() {
		new_test_ext().execute_with(|| {
			let multisig_id = create_test_multisig();
			let not_an_owner = 4; 
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

			assert_noop!(
				Multisig::submit_proposal(
					RuntimeOrigin::signed(not_an_owner),
					multisig_id,
					Box::new(call)
				),
				Error::<Test>::NotAnOwner
			);
		});
	}
}