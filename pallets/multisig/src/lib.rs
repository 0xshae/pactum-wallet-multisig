#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use frame_support::{
		dispatch::{GetDispatchInfo, RawOrigin},
		pallet_prelude::*,
		traits::fungible,
	};
	use frame_system::pallet_prelude::*;
	use sp_io::hashing::blake2_256;
	use sp_runtime::traits::{Dispatchable, TrailingZeroInput};
	use sp_std::prelude::*;

	pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	//CONFIG

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;

		type MaxOwners: Get<u32> + TypeInfo + MaxEncodedLen;
	}
// Custom Types

    /// A unique identifier for a multisig wallet.
    pub type MultisigId = u32;

    /// A unique identifier for a proposal within a specific multisig.
    pub type ProposalIndex = u32;

    /// Represents the on-chain configuration of a multisig wallet.
    ///
    /// This struct bundles the core properties of a wallet into a single, logical unit.
    #[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq)]
    pub struct Multisig<AccountId, MaxOwners: Get<u32>> {
        /// The list of accounts that are owners of this multisig.
        pub owners: BoundedVec<AccountId, MaxOwners>,
        /// The number of owner approvals required to execute a proposal.
        pub threshold: u32,
    }

    /// Represents a pending proposal that owners can confirm.
    ///
    /// This tracks the state of a proposed action.
    #[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq)]
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
    pub type NextMultisigId<T> = StorageValue<_, MultisigId, ValueQuery>;

    /// A map from a `MultisigId` to its on-chain `Multisig` configuration.
    ///
    /// This is the main storage type for storing the wallet configurations.
    #[pallet::storage]
    pub type Multisigs<T: Config> =
        StorageMap<_, Blake2_128Concat, MultisigId, Multisig<T::AccountId, T::MaxOwners>>;


	//EVENTS
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MultisigCreated { creator: T::AccountId, multisig_id: MultisigId, multisig_account: T::AccountId},

	}
// ERRORS
	#[pallet::error]
	pub enum Error<T> {
		NoneValue,
		StorageOverflow,
		TooManyOwners,
		InvalidThreshold,
		
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
        /// Parameters:
        /// - `origin`: The signed account of the user creating the multisig.
        /// - `owners`: A vector of `AccountId`s who will be the owners of the new wallet.
        /// - `threshold`: The number of owner approvals required to execute a proposal.
        ///
        /// Emits `MultisigCreated` on successful creation.
        ///
        /// The origin for this call must be a `Signed` origin.
        #[pallet::call_index(0)]
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
    }


	//HELPER FUNCTIONS
	impl<T: Config> Pallet<T> {
		pub fn next_multisig_id() -> MultisigId {
			<NextMultisigId<T>>::get()
		}

		pub fn multi_account_id(seed: u32) -> T::AccountId {
			let entropy = (b"pba/multisig", seed).using_encoded(blake2_256);
			Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
				.expect("infinite length input; no invalid inputs for type; qed")
		}
	}
}
