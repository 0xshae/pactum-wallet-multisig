use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};

mod create_multisig {
	use super::*;

	#[test]
	fn it_creates_a_multisig_successfully() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let owners = vec![1, 2, 3];
			let threshold = 2;
			let creator = 1;

			assert_ok!(Multisig::create_multisig(RuntimeOrigin::signed(creator), owners.clone(), threshold));

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

