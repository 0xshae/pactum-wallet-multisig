#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weight;
pub use weight::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::*, traits::ReservableCurrency};
	use frame_system::{pallet_prelude::*, RawOrigin};
	use sp_io::hashing::blake2_256;
	use sp_runtime::traits::{Dispatchable, TrailingZeroInput};
	use sp_std::prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	//CONFIG
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: ReservableCurrency<Self::AccountId>;

		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;

		#[pallet::constant]
		type MaxOwners: Get<u32>;

		type WeightInfo: WeightInfo;
	}

	// Custom Types

	/// A unique identifier for a multisig wallet.
	pub type MultisigId = u32;

	/// A unique identifier for a proposal within a specific multisig.
	pub type ProposalIndex = u32;

	/// Represents the on-chain configuration of a multisig wallet.
	///
	/// This struct bundles the core properties of a wallet into a single, logical unit.
	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
	#[scale_info(skip_type_params(MaxOwners))]
	pub struct Multisig<AccountId, MaxOwners: Get<u32>> {
		/// The list of accounts that are owners of this multisig.
		pub owners: BoundedVec<AccountId, MaxOwners>,
		/// The number of owner approvals required to execute a proposal.
		pub threshold: u32,
	}

	/// Represents a pending proposal that owners can confirm.
	///
	/// This tracks the state of a proposed action.
	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
	pub struct Proposal {
		/// The hash of the call to be executed. Storing only the hash is a
		/// storage optimization compared to storing the full call data.
		pub call_hash: [u8; 32],
		/// A flag to track whether the proposal has been successfully executed,
		/// preventing re-execution.
		pub executed: bool,
	}

	// STORAGE

	/// A counter for generating unique multisig IDs.
	#[pallet::storage]
	#[pallet::getter(fn next_multisig_id)]
	pub type NextMultisigId<T> = StorageValue<_, MultisigId, ValueQuery>;

	/// A map from a `MultisigId` to its on-chain `Multisig` configuration.
	///
	/// This is the main storage type for storing the wallet configurations.
	#[pallet::storage]
	#[pallet::getter(fn multisigs)]
	pub type Multisigs<T: Config> =
		StorageMap<_, Blake2_128Concat, MultisigId, Multisig<T::AccountId, T::MaxOwners>>;

	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub type Proposals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		MultisigId,
		Blake2_128Concat,
		ProposalIndex,
		Proposal,
	>;

	#[pallet::storage]
	#[pallet::getter(fn next_proposal_index)]
	pub type NextProposalIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, MultisigId, ProposalIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn approvals)]
	pub type Approvals<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		MultisigId,
		Blake2_128Concat,
		ProposalIndex,
		BoundedVec<T::AccountId, T::MaxOwners>,
		ValueQuery,
	>;

	//EVENTS

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new multisig wallet has been created.
		MultisigCreated {
			/// The account that created the multisig.
			creator: T::AccountId,
			/// The unique ID of the new multisig.
			multisig_id: MultisigId,
			/// The sovereign account address of the new multisig.
			multisig_account: T::AccountId,
		},
		ProposalSubmitted {
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
			call_hash: [u8; 32],
		},
		Confirmation {
			who: T::AccountId,
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
		},
		ProposalExecuted {
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
			result: DispatchResult,
		},
		MultisigDestroyed { multisig_id: MultisigId },
	}

	// ERRORS
	/// Errors that can be returned by this pallet.
	#[pallet::error]
	pub enum Error<T> {
		/// An operation caused a number to overflow.
		StorageOverflow,
		/// The number of owners submitted is greater than the allowed maximum.
		TooManyOwners,
		/// The provided threshold is invalid; it must be greater than 0 and less
		/// than or equal to the number of owners.
		InvalidThreshold,
		MultisigNotFound,
		NotAnOwner,
		ProposalNotFound,
		AlreadyExecuted,
		AlreadyConfirmed,
		MustBeMultisig,
		CallHashMismatch,
		NotEnoughApprovals,
	}

	//CALLS
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates a new, persistent multisig wallet.
		///
		/// This extrinsic allows a signed user to create a new multisig wallet with a
		/// specified set of owners and an approval threshold. Upon successful creation,
		/// the wallet is assigned a unique `MultisigId` and a deterministic, sovereign
		/// `AccountId` is derived. This sovereign account can hold funds and dispatch
		/// calls on behalf of the multisig owners.
		///
		/// ### Parameters:
		/// - `origin`: The signed account of the user creating the multisig.
		/// - `owners`: A vector of `AccountId`s who will be the owners of the new wallet.
		/// - `threshold`: The number of owner approvals required to execute a proposal.
		///
		/// ### Emits:
		/// - `MultisigCreated` on successful creation.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_multisig())]
		pub fn create_multisig(
			origin: OriginFor<T>,
			owners: Vec<T::AccountId>,
			threshold: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Validate that the number of owners does not exceed the configured maximum.
			let bounded_owners: BoundedVec<_, _> =
				owners.try_into().map_err(|_| Error::<T>::TooManyOwners)?;

			// Ensure the threshold is a sensible value.
			ensure!(
				threshold > 0 && threshold <= bounded_owners.len() as u32,
				Error::<T>::InvalidThreshold
			);

			// Generate a new, unique ID for the multisig.
			let multisig_id = Self::next_multisig_id();
			NextMultisigId::<T>::put(
				multisig_id.checked_add(1).ok_or(Error::<T>::StorageOverflow)?,
			);

			// Derive the sovereign account ID for the new multisig.
			let multisig_account = Self::multi_account_id(multisig_id);

			// Create and store the new multisig's configuration.
			let new_multisig = Multisig { owners: bounded_owners, threshold };
			<Multisigs<T>>::insert(multisig_id, new_multisig);

			// Emit an event to notify the outside world of the new multisig.
			Self::deposit_event(Event::MultisigCreated {
				creator: who,
				multisig_id,
				multisig_account,
			});

			Ok(())
		}
		/// Submits a new proposal for a multisig wallet to execute.
		///
		/// This extrinsic can only be called by an owner of the specified multisig.
		/// It creates a new proposal record, storing the hash of the `call` to be
		/// executed. The submitter's account is automatically added as the first
		/// confirmation for the proposal.
		///
		/// ### Parameters:
		/// - `origin`: The signed account of the multisig owner submitting the proposal.
		/// - `multisig_id`: The ID of the multisig for which the proposal is being made.
		/// - `call`: The `RuntimeCall` that the multisig owners will vote on to execute.
		///
		/// ### Emits:
		/// - `ProposalSubmitted` on successful submission.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::submit_proposal())]
		pub fn submit_proposal(
			origin: OriginFor<T>,
			multisig_id: MultisigId,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			let who = ensure_signed(origin)?;
			// Ensure the multisig exists and that the signer is a valid owner.
			let multisig = Self::multisigs(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;
			ensure!(multisig.owners.contains(&who), Error::<T>::NotAnOwner);

			// Generate a new, unique index for this proposal within the scope of the multisig.
			let proposal_index = Self::next_proposal_index(multisig_id);
			NextProposalIndex::<T>::insert(
				multisig_id,
				proposal_index.checked_add(1).ok_or(Error::<T>::StorageOverflow)?,
			);

			// Calculate the hash of the call for storage optimization instead of storing the full
			// call.
			let call_hash = blake2_256(&call.encode());
			let new_proposal = Proposal { call_hash, executed: false };
			<Proposals<T>>::insert(multisig_id, proposal_index, new_proposal);

			// The submitter automatically confirms their own proposal.
			let mut approvals = BoundedVec::new();
			approvals.try_push(who.clone()).map_err(|_| Error::<T>::TooManyOwners)?;
			<Approvals<T>>::insert(multisig_id, proposal_index, approvals);

			// Emit an event to notify users of the new proposal.
			Self::deposit_event(Event::ProposalSubmitted {
				multisig_id,
				proposal_index,
				call_hash,
			});
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::confirm_proposal())]
		pub fn confirm_proposal(
			origin: OriginFor<T>,
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig = Self::multisigs(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;
			ensure!(multisig.owners.contains(&who), Error::<T>::NotAnOwner);
			let proposal =
				Self::proposals(multisig_id, proposal_index).ok_or(Error::<T>::ProposalNotFound)?;
			ensure!(!proposal.executed, Error::<T>::AlreadyExecuted);

			let mut approvals = Self::approvals(multisig_id, proposal_index);
			ensure!(!approvals.contains(&who), Error::<T>::AlreadyConfirmed);

			approvals.try_push(who.clone()).map_err(|_| Error::<T>::TooManyOwners)?;
			<Approvals<T>>::insert(multisig_id, proposal_index, approvals);

			Self::deposit_event(Event::Confirmation { who, multisig_id, proposal_index });
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::execute_proposal())]
		pub fn execute_proposal(
			origin: OriginFor<T>,
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			let multisig = Self::multisigs(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;
			let mut proposal =
				Self::proposals(multisig_id, proposal_index).ok_or(Error::<T>::ProposalNotFound)?;
			ensure!(!proposal.executed, Error::<T>::AlreadyExecuted);

			let call_hash = blake2_256(&call.encode());
			ensure!(proposal.call_hash == call_hash, Error::<T>::CallHashMismatch);

			let approvals = Self::approvals(multisig_id, proposal_index);
			ensure!(approvals.len() as u32 >= multisig.threshold, Error::<T>::NotEnoughApprovals);

			let multisig_account = Self::multi_account_id(multisig_id);
			let result = call.dispatch(RawOrigin::Signed(multisig_account).into());

			// Only mark as executed if the dispatch was successful
			if result.is_ok() {
				proposal.executed = true;
				<Proposals<T>>::insert(multisig_id, proposal_index, proposal);
			}

			Self::deposit_event(Event::ProposalExecuted {
				multisig_id,
				proposal_index,
				result: result.map(|_| ()).map_err(|e| e.error),
			});
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::destroy_multisig())]
		pub fn destroy_multisig(origin: OriginFor<T>, multisig_id: MultisigId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_account = Self::multi_account_id(multisig_id);

			// Only the multisig's own sovereign account can destroy it.
			ensure!(who == multisig_account, Error::<T>::MustBeMultisig);

			// Clean up storage
			<Multisigs<T>>::remove(multisig_id);
			<NextProposalIndex<T>>::remove(multisig_id);
			let _ = <Proposals<T>>::clear_prefix(multisig_id, u32::MAX, None);
			let _ = <Approvals<T>>::clear_prefix(multisig_id, u32::MAX, None);

			Self::deposit_event(Event::MultisigDestroyed { multisig_id });
			Ok(())
		}
	}

	//HELPER FUNCTIONS
	impl<T: Config> Pallet<T> {
		pub fn multi_account_id(seed: u32) -> T::AccountId {
			let entropy = (b"pba/multisig", seed).using_encoded(blake2_256);
			Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
				.expect("infinite length input; no invalid inputs for type; qed")
		}
	}
}
