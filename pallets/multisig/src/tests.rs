// This file contains the unit tests for the multisig pallet.
// Each major extrinsic has its own test module to keep the tests organized.
// The tests follow a standard "Arrange, Act, Assert" pattern:
// 1. Arrange: Set up the initial state of the mock runtime.
// 2. Act: Dispatch the extrinsic being tested.
// 3. Assert: Verify that the state has changed as expected and the correct events were emitted.

use crate::{mock::*, Error, Event, Proposals};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, BoundedVec, dispatch::DispatchResult, traits::Currency};
use sp_io::hashing::blake2_256;

// --- TESTS FOR create_multisig ---
/// Tests for the `create_multisig` extrinsic.
mod create_multisig {
	use super::*;

	/// Tests the successful creation of a new multisig wallet.
	#[test]
	fn it_creates_a_multisig_successfully() {
		new_test_ext().execute_with(|| {
			// Arrange: Set a block number to ensure events are deposited.
			System::set_block_number(1);
			let owners = vec![1, 2, 3];
			let threshold = 2;
			let creator = 1;

			// Act: Dispatch the extrinsic.
			assert_ok!(Multisig::create_multisig(
				RuntimeOrigin::signed(creator),
				owners.clone(),
				threshold
			));

			// Assert: Verify the final state is correct.
			let multisig_id = 0;
			// Check that the multisig was stored correctly with the right owners and threshold.
			let multisig = Multisig::multisigs(multisig_id).unwrap();
			assert_eq!(multisig.owners.to_vec(), owners);
			assert_eq!(multisig.threshold, threshold);
			// Check that the ID counter has been incremented.
			assert_eq!(Multisig::next_multisig_id(), 1);
			// Check that the correct event was emitted.
			let multisig_account = Multisig::multi_account_id(multisig_id);
			System::assert_last_event(
				Event::MultisigCreated { creator, multisig_id, multisig_account }.into(),
			);
		});
	}

	/// Tests that the extrinsic fails if the threshold is zero.
	#[test]
	fn fails_if_threshold_is_zero() {
		new_test_ext().execute_with(|| {
			// Act & Assert: Ensure the extrinsic fails with the correct error.
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), vec![1, 2, 3], 0),
				Error::<Test>::InvalidThreshold
			);
		});
	}

	/// Tests that the extrinsic fails if the threshold is greater than the number of owners.
	#[test]
	fn fails_if_threshold_is_greater_than_owners() {
		new_test_ext().execute_with(|| {
			// Act & Assert: Ensure the extrinsic fails with the correct error.
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), vec![1, 2, 3], 4),
				Error::<Test>::InvalidThreshold
			);
		});
	}

	/// Tests that the extrinsic fails if the number of owners exceeds the configured `MaxOwners`.
	#[test]
	fn fails_if_too_many_owners() {
		new_test_ext().execute_with(|| {
			// Arrange: `MaxOwners` is set to 10 in the mock runtime, so 11 owners should fail.
			let owners = (1..=11).collect::<Vec<u64>>();

			// Act & Assert: Ensure the extrinsic fails with the correct error.
			assert_noop!(
				Multisig::create_multisig(RuntimeOrigin::signed(1), owners, 2),
				Error::<Test>::TooManyOwners
			);
		});
	}
}

/// Tests for the `submit_proposal` extrinsic.
mod submit_proposal {
	use super::*;

	/// A helper function to create a standard multisig for use in other tests.
	fn create_test_multisig() -> u32 {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
		0 // Returns the ID of the created multisig.
	}

	/// Tests the successful submission of a new proposal.
	#[test]
	fn it_submits_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			// Arrange
			System::set_block_number(1);
			let multisig_id = create_test_multisig();
			let proposer = 1; // An owner of the multisig.
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![0; 10] }.into();
			let call_hash = blake2_256(&call.encode());

			// Act: Dispatch the extrinsic.
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(proposer),
				multisig_id,
				Box::new(call)
			));

			// Assert
			let proposal_index = 0;
			// Check that the proposal was stored with the correct hash and is not executed.
			let proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			assert_eq!(proposal.call_hash, call_hash);
			assert!(!proposal.executed);
			// Check that the proposer's approval was automatically recorded.
			let expected_approvals: BoundedVec<u64, <Test as crate::Config>::MaxOwners> =
				vec![proposer].try_into().unwrap();
			assert_eq!(Multisig::approvals(multisig_id, proposal_index), expected_approvals);
			// Check that the proposal index counter for this multisig was incremented.
			assert_eq!(Multisig::next_proposal_index(multisig_id), 1);
			// Check that the correct event was emitted.
			System::assert_last_event(
				Event::ProposalSubmitted { multisig_id, proposal_index, call_hash }.into(),
			);
		});
	}

	/// Tests that the extrinsic fails if the specified multisig does not exist.
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

	/// Tests that the extrinsic fails if the caller is not an owner of the multisig.
	#[test]
	fn fails_if_not_an_owner() {
		new_test_ext().execute_with(|| {
			let multisig_id = create_test_multisig();
			let not_an_owner = 4; // Owners are [1, 2, 3].
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

/// Tests for the `confirm_proposal` extrinsic.
mod confirm_proposal {
	use super::*;

	/// A helper function to create a multisig with a pending proposal.
	fn setup_multisig_with_proposal() -> (u32, u32) {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		let proposer = 1;
		let call: RuntimeCall = frame_system::Call::remark { remark: vec![0; 10] }.into();

		// Create the multisig.
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(proposer), owners, threshold));
		let multisig_id = 0;

		// Submit the proposal.
		assert_ok!(Multisig::submit_proposal(
			RuntimeOrigin::signed(proposer),
			multisig_id,
			Box::new(call)
		));
		let proposal_index = 0;

		(multisig_id, proposal_index)
	}

	/// Tests the successful confirmation of a pending proposal.
	#[test]
	fn it_confirms_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			// Arrange
			System::set_block_number(1);
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();
			let confirmer = 2; // Another owner.

			// Act: Dispatch the extrinsic to confirm.
			assert_ok!(Multisig::confirm_proposal(
				RuntimeOrigin::signed(confirmer),
				multisig_id,
				proposal_index
			));

			// Assert
			// Verify both the original proposer (1) and the new confirmer (2) are in the approvals list.
			let expected_approvals: BoundedVec<u64, <Test as crate::Config>::MaxOwners> =
				vec![1, 2].try_into().unwrap();
			assert_eq!(Multisig::approvals(multisig_id, proposal_index), expected_approvals);
			// Verify that the correct event was emitted.
			System::assert_last_event(
				Event::Confirmation { who: confirmer, multisig_id, proposal_index }.into(),
			);
		});
	}

	/// Tests that the extrinsic fails if an owner tries to confirm a proposal they have already approved.
	#[test]
	fn fails_if_already_confirmed() {
		new_test_ext().execute_with(|| {
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();
			let proposer = 1; // The original proposer.

			// Act & Assert: The proposer tries to confirm their own proposal again.
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

	/// Tests that the extrinsic fails if the specified proposal does not exist.
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

	/// Tests that the extrinsic fails if the proposal has already been executed.
	#[test]
	fn fails_if_proposal_already_executed() {
		new_test_ext().execute_with(|| {
			// Arrange: Set up a proposal and then manually mark it as executed.
			let (multisig_id, proposal_index) = setup_multisig_with_proposal();
			let mut proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			proposal.executed = true;
			Proposals::<Test>::insert(multisig_id, proposal_index, proposal);

			// Act & Assert: Another owner tries to confirm the now-executed proposal.
			assert_noop!(
				Multisig::confirm_proposal(
					RuntimeOrigin::signed(2),
					multisig_id,
					proposal_index
				),
				Error::<Test>::AlreadyExecuted
			);
		});
	}
}

/// Tests for the `execute_proposal` extrinsic.
mod execute_proposal {
	use super::*;

	/// A helper function to set up a proposal that has met its threshold and is ready to be executed.
	fn setup_ready_to_execute_proposal() -> (u32, u32, RuntimeCall) {
		let owners = vec![1, 2, 3];
		let threshold = 2;
		let proposer = 1;
		let confirmer = 2;
		let call: RuntimeCall = frame_system::Call::remark_with_event { remark: vec![42] }.into();

		// Create the multisig.
		assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(proposer), owners, threshold));
		let multisig_id = 0;

		// Submit the proposal.
		assert_ok!(Multisig::submit_proposal(
			RuntimeOrigin::signed(proposer),
			multisig_id,
			Box::new(call.clone())
		));
		let proposal_index = 0;

		// Confirm the proposal to meet the threshold.
		assert_ok!(Multisig::confirm_proposal(
			RuntimeOrigin::signed(confirmer),
			multisig_id,
			proposal_index
		));

		(multisig_id, proposal_index, call)
	}

	/// Tests the successful execution of a fully approved proposal.
	#[test]
	fn it_executes_a_proposal_successfully() {
		new_test_ext().execute_with(|| {
			// Arrange
			System::set_block_number(1);
			let (multisig_id, proposal_index, call) = setup_ready_to_execute_proposal();
			let executor = 4; // Anyone can execute.

			// Act: Dispatch the extrinsic.
			assert_ok!(Multisig::execute_proposal(
				RuntimeOrigin::signed(executor),
				multisig_id,
				proposal_index,
				Box::new(call.clone())
			));

			// Assert
			// Verify the proposal is now marked as executed in storage.
			let proposal = Multisig::proposals(multisig_id, proposal_index).unwrap();
			assert!(proposal.executed);
			// Verify the `ProposalExecuted` event was emitted with a successful result.
			let result: DispatchResult = Ok(().into());
			System::assert_last_event(
				Event::ProposalExecuted { multisig_id, proposal_index, result }.into(),
			);
			// Verify that the inner call (`remark_with_event`) was actually dispatched by checking for its specific event.
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

	/// Tests that the extrinsic fails if the approval threshold has not been met.
	#[test]
	fn fails_if_not_enough_approvals() {
		new_test_ext().execute_with(|| {
			// Arrange: A proposal is submitted but not confirmed, so it only has 1 approval, but needs 2.
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;
			let call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(1),
				multisig_id,
				Box::new(call.clone())
			));
			let proposal_index = 0;

			// Act & Assert
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

	/// Tests that the extrinsic fails if the provided call does not match the approved proposal's hash.
	#[test]
	fn fails_if_call_hash_mismatches() {
		new_test_ext().execute_with(|| {
			// Arrange: Set up a ready-to-execute proposal for one call.
			let (multisig_id, proposal_index, _call) = setup_ready_to_execute_proposal();
			// Create a different call to try and execute instead.
			let different_call: RuntimeCall =
				frame_system::Call::remark { remark: vec![99] }.into();

			// Act & Assert
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


/// Tests for the `destroy_multisig` extrinsic.
mod destroy_multisig {
	use super::*;

	/// Tests the successful destruction of a multisig via the self-governance mechanism.
	#[test]
	fn it_destroys_a_multisig_successfully() {
		new_test_ext().execute_with(|| {
			// Arrange
			System::set_block_number(1);
			// Create a multisig.
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;

			// Act: The owners must propose, confirm, and execute the destruction of their own wallet.
			// 1. Propose the destruction. The call to be executed IS the destroy_multisig call itself.
			let destroy_call: RuntimeCall = crate::Call::destroy_multisig { multisig_id }.into();
			assert_ok!(Multisig::submit_proposal(
				RuntimeOrigin::signed(1), // proposer
				multisig_id,
				Box::new(destroy_call.clone())
			));
			let proposal_index = 0;
			// 2. Confirm the destruction proposal to meet the threshold.
			assert_ok!(Multisig::confirm_proposal(
				RuntimeOrigin::signed(2), // confirmer
				multisig_id,
				proposal_index
			));
			// 3. Execute the destruction proposal.
			assert_ok!(Multisig::execute_proposal(
				RuntimeOrigin::signed(3), // Can be anyone
				multisig_id,
				proposal_index,
				Box::new(destroy_call)
			));

			// Assert: Verify that all storage related to the multisig has been cleaned up.
			assert!(Multisig::multisigs(multisig_id).is_none());
			assert!(Multisig::proposals(multisig_id, proposal_index).is_none());
			assert!(Multisig::approvals(multisig_id, proposal_index).is_empty());
			assert_eq!(Multisig::next_proposal_index(multisig_id), 0);
			// Verify the destruction event was emitted.
			System::assert_has_event(Event::MultisigDestroyed { multisig_id }.into());
		});
	}

	/// Tests that the extrinsic fails if called directly by any account other than the multisig's own sovereign account.
	#[test]
	fn fails_if_origin_is_not_sovereign_account() {
		new_test_ext().execute_with(|| {
			// Arrange: Create a multisig.
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;

			// Act & Assert: A regular user (even an owner) tries to call destroy_multisig directly.
			assert_noop!(
				Multisig::destroy_multisig(RuntimeOrigin::signed(1), multisig_id),
				Error::<Test>::MustBeMultisig
			);
		});
	}

	/// Tests that destruction is blocked if the multisig's sovereign account still holds a balance.
	#[test]
	fn fails_if_balance_is_not_zero() {
		new_test_ext().execute_with(|| {
			// Arrange
			System::set_block_number(1);
			let owners = vec![1, 2, 3];
			let threshold = 2;
			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(1), owners, threshold));
			let multisig_id = 0;
			let multisig_account = Multisig::multi_account_id(multisig_id);
			// Fund the multisig account so it has a non-zero balance.
			let _ = Balances::deposit_creating(&multisig_account, 100);

			// Act: Propose, confirm, and attempt to execute the destruction.
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
			
		
			// The outer `execute_proposal` succeeds, but the inner `destroy_multisig` fails.
			// We check that the `ProposalExecuted` event was emitted with the correct inner error.
			let result: DispatchResult = Err(Error::<Test>::NonZeroBalance.into());
			System::assert_last_event(
				Event::ProposalExecuted { multisig_id, proposal_index, result }.into(),
			);
			// Verify that the multisig was NOT destroyed because of the failure.
			assert!(Multisig::multisigs(multisig_id).is_some());
		});
	}
}