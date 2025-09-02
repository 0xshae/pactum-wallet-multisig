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

		type MaxOwners: Get<u32>;

	}

	//Custom Types

	pub type MultisigId = u32;
	pub type ProposalIndex = u32;

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq)]
    pub struct Multisig<AccountId, MaxOwners: Get<u32>> {
        pub owners: BoundedVec<AccountId, MaxOwners>,
        pub threshold: u32,
    }

    #[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq)]
    pub struct Proposal {
        pub call_hash: [u8; 32],
        pub executed: bool,
    }


	//STORAGE

	#[pallet::storage]
	pub type Something<T> = StorageValue<Value = u32>;
	#[pallet::storage]
	pub type SomethingMap<T: Config> = StorageMap<Key = T::AccountId, Value = BlockNumberFor<T>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		SomethingStored { something: u32, who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		NoneValue,
		StorageOverflow,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn do_something(origin: OriginFor<T>, something: u32) -> DispatchResult {
			let who = ensure_signed(origin)?;

			<Something<T>>::put(something);

			Self::deposit_event(Event::SomethingStored { something, who });

			Ok(())
		}

		pub fn cause_error(origin: OriginFor<T>) -> DispatchResult {
			let _who = ensure_signed(origin)?;

			match <Something<T>>::get() {
				None => Err(Error::<T>::NoneValue.into()),
				Some(old) => {
					let new = old.checked_add(1).ok_or(Error::<T>::StorageOverflow)?;
					<Something<T>>::put(new);
					Ok(())
				},
			}
		}

		pub fn redispatch(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let res = call.dispatch(RawOrigin::Signed(who).into());

			res.map(|_| ()).map_err(|e| e.error)
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn multi_account_id(seed: u32) -> T::AccountId {
			let entropy = (b"pba/multisig", seed).using_encoded(blake2_256);
			Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
				.expect("infinite length input; no invalid inputs for type; qed")
		}
	}
}
