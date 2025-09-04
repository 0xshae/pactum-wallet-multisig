use crate::{mock::*, Error, Event, Proposals};
use codec::Encode;
use frame_support::{
	assert_noop, assert_ok, dispatch::DispatchResult, traits::Currency, BoundedVec,
};
use sp_io::hashing::blake2_256;

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
			let expected_approvals: BoundedVec<u64, <Test as crate::Config>::MaxOwners> =
				vec![proposer].try_into().unwrap();
			assert_eq!(Multisig::approvals(multisig_id, proposal_index), expected_approvals);

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

mod confirm_proposal {
	use super::*;

	/// Helper function to create a multisig with a pending proposal.
	/// Returns (multisig_id, proposal_index).
	fn setup_multisig_with_proposal() -> (u32, u32) {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		let proposer = 1;
		let call: RuntimeCall = frame_system::Call::remark { remark: vec![0; 10] }.into();

		// Create the multisig
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(proposer), owners, threshold));
		let multisig_id = 0;

		// Submit the proposal
		assert_ok!(Multisig::submit_proposal(
			RuntimeOrigin::signed(proposer),
			multisig_id,
			Box::new(call)
		));
		let proposal_index = 0;

		(multisig_id, proposal_index)
	}

	#[test]
	fn it_confirms_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();
			let confirmer = 2; // Another owner

			// Dispatch the extrinsic to confirm
			assert_ok!(Multisig::confirm_proposal(
				RuntimeOrigin::signed(confirmer),
				multisig_id,
				proposal_index
			));

			// Verify storage state: both proposer (1) and confirmer (2) should be in approvals
			let expected_approvals: BoundedVec<u64, <Test as crate::Config>::MaxOwners> =
				vec![1, 2].try_into().unwrap();
			assert_eq!(Multisig::approvals(multisig_id, proposal_index), expected_approvals);

			// Verify that the correct event was emitted
			System::assert_last_event(
				Event::Confirmation { who: confirmer, multisig_id, proposal_index }.into(),
			);
		});
	}

	#[test]
	fn fails_if_already_confirmed() {
		new_test_ext().execute_with(|| {
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();
			let proposer = 1; // The original proposer

			// The proposer tries to confirm again
			assert_noop!(
				Multisig::confirm_proposal(
					RuntimeOrigin::signed(proposer),
					multisig_id,
					proposal_index
				),
				Error::<Test>::AlreadyConfirmed
			);
		});
	}

	#[test]
	fn fails_if_proposal_not_found() {
		new_test_ext().execute_with(|| {
			let (multisig_id, _) = setup_multisig_with_proposal();
			let non_existent_proposal_index = 99;

			assert_noop!(
				Multisig::confirm_proposal(
					RuntimeOrigin::signed(2),
					multisig_id,
					non_existent_proposal_index
				),
				Error::<Test>::ProposalNotFound
			);
		});
	}

	#[test]
	fn fails_if_proposal_already_executed() {
		new_test_ext().execute_with(|| {
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();

			// Manually set the proposal to be executed
			let mut proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			proposal.executed = true;
			Proposals::<Test>::insert(multisig_id, proposal_index, proposal);

			// Another owner tries to confirm the now-executed proposal
			assert_noop!(
				Multisig::confirm_proposal(RuntimeOrigin::signed(2), multisig_id, proposal_index),
				Error::<Test>::AlreadyExecuted
			);
		});
	}
}

mod execute_proposal {
	use super::*;
	use frame_support::dispatch::DispatchResult;

	/// Helper function to set up a proposal that is ready to be executed.
	/// Returns (multisig_id, proposal_index, call_to_execute).
	fn setup_ready_to_execute_proposal() -> (u32, u32, RuntimeCall) {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		let proposer = 1;
		let confirmer = 2;
		let call: RuntimeCall = frame_system::Call::remark_with_event { remark: vec![42] }.into();

		// Create the multisig
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(proposer), owners, threshold));
		let multisig_id = 0;

		// Submit the proposal
		assert_ok!(Multisig::submit_proposal(
			RuntimeOrigin::signed(proposer),
			multisig_id,
			Box::new(call.clone())
		));
		let proposal_index = 0;

		// Confirm the proposal to meet the threshold
		assert_ok!(Multisig::confirm_proposal(
			RuntimeOrigin::signed(confirmer),
			multisig_id,
			proposal_index
		));

		(multisig_id, proposal_index, call)
	}

	#[test]
	fn it_executes_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let (multisig_id, proposal_index, call) = setup_ready_to_execute_proposal();
			let executor = 4;

			// Dispatch the extrinsic
			assert_ok!(Multisig::execute_proposal(
				RuntimeOrigin::signed(executor),
				multisig_id,
				proposal_index,
				Box::new(call.clone())
			));

			// Verify storage: proposal should be marked as executed
			let proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			assert!(proposal.executed);

			// Verify event emission
			let result: DispatchResult = Ok(().into());
			System::assert_last_event(
				Event::ProposalExecuted { multisig_id, proposal_index, result }.into(),
			);

			// Verify that the inner call (remark_with_event) was actually dispatched
			// by checking for its specific event.
			let multisig_account = Multisig::multi_account_id(multisig_id);
			let remark_hash = blake2_256(&vec![42]);
			System::assert_has_event(
				frame_system::Event::Remarked {
					sender: multisig_account,
					hash: remark_hash.into(),
				}
				.into(),
			);
		});
	}

	#[test]
	fn fails_if_not_enough_approvals() {
		new_test_ext().execute_with(|| {
			// Setup a multisig with a proposal, but don't confirm it enough
			let owners = vec![1, 2, 3];
			let threshold = 2; // Needs 2 approvals
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(1),
				multisig_id,
				Box::new(call.clone())
			));
			let proposal_index = 0;

			// Only 1 approval exists, but threshold is 2
			assert_noop!(
				Multisig::execute_proposal(
					RuntimeOrigin::signed(1),
					multisig_id,
					proposal_index,
					Box::new(call)
				),
				Error::<Test>::NotEnoughApprovals
			);
		});
	}

	#[test]
	fn fails_if_call_hash_mismatches() {
		new_test_ext().execute_with(|| {
			let (multisig_id, proposal_index, _call) = setup_ready_to_execute_proposal();

			// Create a different call to try and execute
			let different_call: RuntimeCall =
				frame_system::Call::remark { remark: vec![99] }.into();

			assert_noop!(
				Multisig::execute_proposal(
					RuntimeOrigin::signed(1),
					multisig_id,
					proposal_index,
					Box::new(different_call)
				),
				Error::<Test>::CallHashMismatch
			);
		});
	}
}

mod destroy_multisig {
	use super::*;

	#[test]
	fn it_destroys_a_multisig_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;

			let destroy_call: RuntimeCall = crate::Call::destroy_multisig { multisig_id }.into();
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(1),
				multisig_id,
				Box::new(destroy_call.clone())
			));
			let proposal_index = 0;

			assert_ok!(Multisig::confirm_proposal(
				RuntimeOrigin::signed(2),
				multisig_id,
				proposal_index
			));

			assert_ok!(Multisig::execute_proposal(
				RuntimeOrigin::signed(3),
				multisig_id,
				proposal_index,
				Box::new(destroy_call)
			));

			assert!(Multisig::multisigs(multisig_id).is_none());
			assert!(Multisig::proposals(multisig_id, proposal_index).is_none());
			assert!(Multisig::approvals(multisig_id, proposal_index).is_empty());
			assert_eq!(Multisig::next_proposal_index(multisig_id), 0);

			System::assert_has_event(Event::MultisigDestroyed { multisig_id }.into());
		});
	}

	#[test]
	fn fails_if_origin_is_not_sovereign_account() {
		new_test_ext().execute_with(|| {
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;

			assert_noop!(
				Multisig::destroy_multisig(RuntimeOrigin::signed(1), multisig_id),
				Error::<Test>::MustBeMultisig
			);
		});
	}

	#[test]
	fn fails_if_balance_is_not_zero() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;
			let multisig_account = Multisig::multi_account_id(multisig_id);

			let _ = Balances::deposit_creating(&multisig_account, 100);

			let destroy_call: RuntimeCall = crate::Call::destroy_multisig { multisig_id }.into();
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(1),
				multisig_id,
				Box::new(destroy_call.clone())
			));
			let proposal_index = 0;
			assert_ok!(Multisig::confirm_proposal(
				RuntimeOrigin::signed(2),
				multisig_id,
				proposal_index
			));

			assert_ok!(Multisig::execute_proposal(
				RuntimeOrigin::signed(3),
				multisig_id,
				proposal_index,
				Box::new(destroy_call)
			));

			let result: DispatchResult = Err(Error::<Test>::NonZeroBalance.into());
			System::assert_last_event(
				Event::ProposalExecuted { multisig_id, proposal_index, result }.into(),
			);

			assert!(Multisig::multisigs(multisig_id).is_some());
		});
	}
}
