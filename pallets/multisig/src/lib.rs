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

/// A pallet for creating and managing stateful, on-chain multisig wallets.

///
/// The core lifecycle is as follows:
/// 1. **Creation:** A user creates a wallet with a set of owners and an approval threshold.
/// 2. **Proposal:** An owner proposes a `RuntimeCall` for the group to approve.
/// 3. **Confirmation:** Other owners confirm the proposal until the threshold is met.
/// 4. **Execution:** Once the threshold is met, the proposal's `call` is dispatched from
///    the wallet's own sovereign account.
/// 5. **Destruction:** The wallet can be safely destroyed through a self-governed proposal,
///    ensuring all associated storage is cleaned up.
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::GetDispatchInfo,
		pallet_prelude::*,
		traits::{Currency, ReservableCurrency},
	};
	use frame_system::{pallet_prelude::*, RawOrigin};
	use sp_io::hashing::blake2_256;
	use sp_runtime::traits::{Dispatchable, TrailingZeroInput};
	use sp_std::prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type for the runtime.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency trait for managing balances.
		///
		/// The name `Currency` is used by convention over the boilerplate's `NativeBalance`
		/// to improve clarity and align with standard FRAME development practices.
		type Currency: ReservableCurrency<Self::AccountId>;

		/// The overarching call type for the runtime.
		/// This allows a multisig to propose and dispatch calls from any other pallet.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;

		/// A configurable constant for the maximum number of owners a multisig wallet can have.
		/// This is a security measure to prevent abuse and ensure predictable performance.
		#[pallet::constant]
		type MaxOwners: Get<u32>;

		/// Weight information for extrinsics in this pallet.
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
	// This attribute is a compile-time optimization that tells the `TypeInfo` derive macro
	// to ignore the `MaxOwners` generic parameter. This is necessary because the concrete
	// type used for this in the runtime (`ConstU32`) does not itself implement `TypeInfo`.
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
		/// The hash of the call to be executed.
		///
		///    Storing only the hash of the call is a significant storage
		/// optimization. The full call data is provided by the user again during the
		/// execution phase, where its hash is verified against this stored value.
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
	/// This is the primary storage item for the wallets themselves.
	#[pallet::storage]
	#[pallet::getter(fn multisigs)]
	pub type Multisigs<T: Config> =
		StorageMap<_, Blake2_128Concat, MultisigId, Multisig<T::AccountId, T::MaxOwners>>;

	/// A map to store pending proposals, keyed by the multisig ID and a unique proposal index.
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

	/// A counter for generating unique proposal indices for each multisig.
	///
	/// Each multisig maintains its own separate proposal count to keep indices small and manageable.
	#[pallet::storage]
	#[pallet::getter(fn next_proposal_index)]
	pub type NextProposalIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, MultisigId, ProposalIndex, ValueQuery>;

	/// A map to store the set of accounts that have approved a specific proposal.
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
	/// Events emitted by this pallet.
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
		/// A new proposal has been submitted.
		ProposalSubmitted {
			/// The ID of the multisig the proposal belongs to.
			multisig_id: MultisigId,
			/// The unique index of the new proposal.
			proposal_index: ProposalIndex,
			/// The hash of the proposed call.
			call_hash: [u8; 32],
		},
		/// An owner has confirmed a proposal.
		Confirmation {
			/// The owner who cast the confirmation vote.
			who: T::AccountId,
			/// The ID of the multisig the proposal belongs to.
			multisig_id: MultisigId,
			/// The index of the proposal being confirmed.
			proposal_index: ProposalIndex,
		},
		/// A proposal has been executed.
		ProposalExecuted {
			/// The ID of the multisig the proposal belonged to.
			multisig_id: MultisigId,
			/// The index of the proposal that was executed.
			proposal_index: ProposalIndex,
			/// The result of the dispatched call.
			result: DispatchResult,
		},
		/// A multisig wallet has been destroyed.
		MultisigDestroyed {
			/// The ID of the multisig that was destroyed.
			multisig_id: MultisigId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An operation caused a number to overflow.
		StorageOverflow,
		/// The number of owners submitted is greater than the allowed maximum.
		TooManyOwners,
		/// The provided threshold is invalid; it must be greater than 0 and less
		/// than or equal to the number of owners.
		InvalidThreshold,
		/// The specified multisig wallet does not exist.
		MultisigNotFound,
		/// The caller is not an owner of the multisig.
		NotAnOwner,
		/// The specified proposal does not exist.
		ProposalNotFound,
		/// The proposal has already been executed.
		AlreadyExecuted,
		/// The caller has already confirmed this proposal.
		AlreadyConfirmed,
		/// The origin of the call is not the multisig's own sovereign account.
		MustBeMultisig,
		/// The hash of the provided call does not match the hash of the proposal.
		CallHashMismatch,
		/// The proposal does not have enough approvals to be executed.
		NotEnoughApprovals,
		/// The multisig cannot be destroyed because it still holds a balance.
		NonZeroBalance,
	}


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

			//   Convert the user-provided `Vec` into a `BoundedVec` to prevent a
			// potential DoS attack where a user provides an extremely large number of owners,
			// bloating storage and computation. This fails if `owners.len() > MaxOwners`.
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
			let who = ensure_signed(origin)?;
			let multisig = Self::multisigs(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;
			//  This is the core authorization check, ensuring only owners can create proposals.
			ensure!(multisig.owners.contains(&who), Error::<T>::NotAnOwner);

			// Generate a new, unique index for this proposal within the scope of the multisig.
			let proposal_index = Self::next_proposal_index(multisig_id);
			NextProposalIndex::<T>::insert(
				multisig_id,
				proposal_index.checked_add(1).ok_or(Error::<T>::StorageOverflow)?,
			);

			// Calculate the hash of the call for storage optimization.
			let call_hash = blake2_256(&call.encode());
			let new_proposal = Proposal { call_hash, executed: false };
			<Proposals<T>>::insert(multisig_id, proposal_index, new_proposal);

			//    The submitter automatically confirms their own proposal. This improves
			// UX by saving them from sending a second, separate `confirm_proposal` transaction.
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

		/// Confirms a pending proposal.
		///
		/// This extrinsic can only be called by an owner of the specified multisig who has not
		/// yet confirmed the proposal.
		///
		/// ### Parameters:
		/// - `origin`: The signed account of the owner confirming the proposal.
		/// - `multisig_id`: The ID of the multisig the proposal belongs to.
		/// - `proposal_index`: The index of the proposal being confirmed.
		///
		/// ### Emits:
		/// - `Confirmation` on successful confirmation.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::confirm_proposal())]
		pub fn confirm_proposal(
			origin: OriginFor<T>,
			multisig_id: MultisigId,
			proposal_index: ProposalIndex,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			
			// By checking conditions in this order (cheapest to most expensive), we can
			// fail early and save computational resources if a condition is not met.
			let multisig = Self::multisigs(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;
			ensure!(multisig.owners.contains(&who), Error::<T>::NotAnOwner);
			let proposal =
				Self::proposals(multisig_id, proposal_index).ok_or(Error::<T>::ProposalNotFound)?;
			ensure!(!proposal.executed, Error::<T>::AlreadyExecuted);

			// Perform a read-modify-write operation on the approvals.
			let mut approvals = Self::approvals(multisig_id, proposal_index);
			//  This check prevents a single owner from confirming the same proposal
			// multiple times to artificially meet the threshold.
			ensure!(!approvals.contains(&who), Error::<T>::AlreadyConfirmed);

			approvals.try_push(who.clone()).map_err(|_| Error::<T>::TooManyOwners)?;
			<Approvals<T>>::insert(multisig_id, proposal_index, approvals);

			Self::deposit_event(Event::Confirmation { who, multisig_id, proposal_index });
			Ok(())
		}

		/// Executes a proposal that has met its confirmation threshold.
		///
		/// This extrinsic can be called by any signed account, as the authorization is
		/// based on the on-chain confirmation state, not the caller's identity.
		///
		/// ### Parameters:
		/// - `origin`: Any signed account.
		/// - `multisig_id`: The ID of the multisig the proposal belongs to.
		/// - `proposal_index`: The index of the proposal to be executed.
		/// - `call`: The full `RuntimeCall` corresponding to the proposal's stored hash.
		///
		/// ### Emits:
		/// - `ProposalExecuted` with the result of the dispatched call.
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

			//  Verify that the provided call matches the one that was approved.
			// This prevents a user from tricking owners into approving one action and then
			// executing another, different action.
			let call_hash = blake2_256(&call.encode());
			ensure!(proposal.call_hash == call_hash, Error::<T>::CallHashMismatch);

			// The core authorization check: has the threshold been met?
			let approvals = Self::approvals(multisig_id, proposal_index);
			ensure!(approvals.len() as u32 >= multisig.threshold, Error::<T>::NotEnoughApprovals);

			// Dispatch the call from the multisig's sovereign account.
			let multisig_account = Self::multi_account_id(multisig_id);
			let result = call.dispatch(RawOrigin::Signed(multisig_account).into());

			//   Only update the proposal's state if the dispatch was successful.
			// The second condition is a critical safety check to handle the edge case where the
			// executed call was `destroy_multisig`. In that case, the multisig no longer
			// exists, and we must not attempt to write to its storage again.
			if result.is_ok() && <Multisigs<T>>::contains_key(multisig_id) {
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

		/// Destroys a multisig wallet and cleans up all associated storage.
		///
		///  This is a sovereign action. It can only be called successfully
		/// if the `origin` is the multisig's own derived `AccountId`. This means that
		/// to destroy a wallet, the owners must first propose, confirm, and execute a
		/// call to this very extrinsic.
		///
		/// ### Parameters:
		/// - `origin`: The sovereign `AccountId` of the multisig being destroyed.
		/// - `multisig_id`: The ID of the multisig to destroy.
		///
		/// ### Emits:
		/// - `MultisigDestroyed` on successful destruction.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::destroy_multisig())]
		pub fn destroy_multisig(origin: OriginFor<T>, multisig_id: MultisigId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let multisig_account = Self::multi_account_id(multisig_id);

			// The core security check for this extrinsic.
			ensure!(who == multisig_account, Error::<T>::MustBeMultisig);

			// A check to ensure we are not trying to destroy a non-existent multisig.
			// Prefixed with `_` to silence the "unused variable" warning, as its only
			// purpose is to exist for this check.
			let _multisig = <Multisigs<T>>::get(multisig_id).ok_or(Error::<T>::MultisigNotFound)?;

			//   This is a critical safety net to prevent the accidental, irreversible
			// loss of funds. It forces the owners to first empty the wallet's balance
			// via a separate proposal before they can destroy it.
			let balance = T::Currency::free_balance(&multisig_account);
			ensure!(balance.is_zero(), Error::<T>::NonZeroBalance);

			// Clean up all storage associated with the multisig.
			//    `clear_prefix` is used for convenience to clean up all proposals
			// and approvals in a single action. While this has a variable weight, the sovereign
			// security model ensures this potentially expensive operation is a deliberate,
			// multi-approved decision.
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
		/// Derives a unique, deterministic account ID for a multisig wallet.
		///
		// This function is the cornerstone of the stateful design. It uses the multisig's
		// unique `seed` (its `MultisigId`) and a constant namespace to generate a 32-byte
		// hash, which is then decoded into a valid `AccountId`. This allows the pallet
		// to programmatically control an on-chain account.
		pub fn multi_account_id(seed: u32) -> T::AccountId {
			let entropy = (b"pba/multisig", seed).using_encoded(blake2_256);
			Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
				.expect("infinite length input; no invalid inputs for type; qed")
		}
	}
}