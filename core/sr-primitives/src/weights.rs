// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives for transaction weighting.
//!
//! Each dispatch function within `decl_module!` can have an optional `#[weight = $x]` attribute.
//!  $x can be any object that implements the `Weighable` trait. By default, All transactions are
//! annotated by `#[weight = TransactionWeight::default()]`.
//!
//! Note that the decl_module macro _cannot_ enforce this and will simply fail if an invalid struct
//! (something that does not  implement `Weighable`) is passed in.

use crate::codec::{Decode, Encode};
use crate::Perbill;
use crate::traits::Zero;

/// The final type that each `#[weight = $x:expr]`'s
/// expression must evaluate to.
pub type Weight = u32;

/// Maximum block saturation: 4mb
pub const MAX_TRANSACTIONS_WEIGHT: u32 = 4 * 1024 * 1024;
/// Target block saturation: 25% of max block saturation = 1mb
pub const IDEAL_TRANSACTIONS_WEIGHT: u32 = 1024 * 1024;

/// A `Call` enum (aka transaction) that can be weighted using the custom weight attribute of
/// its dispatchable functions. Is implemented by default in the `decl_module!`.
///
/// Both the outer Call enum and the per-module individual ones will implement this.
/// The outer enum simply calls the inner ones based on call type.
pub trait Weighable {
	/// Return the weight of this call.
	/// The `len` argument is the encoded length of the transaction/call.
	fn weight(&self, len: usize) -> Weight;
}

/// Default type used as the weight representative in a `#[weight = x]` attribute.
///
/// A user may pass in any other type that implements [`Weighable`]. If not, the `Default`
/// implementation of [`TransactionWeight`] is used.
pub enum TransactionWeight {
	/// Basic weight (base, byte).
	/// The values contained are the base weight and byte weight respectively.
	Basic(Weight, Weight),
	/// Maximum fee. This implies that this transaction _might_ get included but
	/// no more transaction can be added. This can be done by setting the
	/// implementation to _maximum block weight_.
	Max,
	/// Free. The transaction does not increase the total weight
	/// (i.e. is not included in weight calculation).
	Free,
}

impl Weighable for TransactionWeight {
	fn weight(&self, len: usize) -> Weight {
		match self {
			TransactionWeight::Basic(base, byte) => base + byte * len as Weight,
			TransactionWeight::Max => 3 * 1024 * 1024,
			TransactionWeight::Free => 0,
		}
	}
}

impl Default for TransactionWeight {
	fn default() -> Self {
		// This implies that the weight is currently equal to tx-size, nothing more
		// for all substrate transactions that do NOT explicitly annotate weight.
		// TODO #2431 needs to be updated with proper max values.
		TransactionWeight::Basic(0, 1)
	}
}

/// A wrapper for fee multiplier.
/// This is to simulate a `Perbill` in the range [-1, infinity]
///
/// The fee multiplier is always multiplied by the weight (as denoted by `TransactionWeight` on a
/// per-transaction basis with `#[weight]` annotation) of the transaction to obtain the final fee.
///
/// One can define how this conversion evolves based on the previous block weight by implementing
/// the `FeeMultiplierUpdate` type of the `system` trait.
#[cfg_attr(feature = "std", derive(PartialEq, Eq, Debug))]
#[derive(Encode, Decode, Clone, Copy)]
pub enum FeeMultiplier {
	/// Should be interpreted as a positive ratio added to the weight, i.e. `weight + weight * p`
	/// where `p` is a small `Perbill`.
	///
	Positive(Perbill, Weight),
	/// Should be interpreted as a negative ratio subtracted from the weight, i.e.
	/// `weight - weight * p` where `p` is a small `Perbill`.
	Negative(Perbill),
}

impl FeeMultiplier {
	/// Applies the self, as a multiplier, to the given weight.
	pub fn apply_to(&self, weight: Weight) -> Weight {
		match *self {
			FeeMultiplier::Positive(p, r) => weight + weight.saturating_mul(r).saturating_add(p * weight),
			FeeMultiplier::Negative(p) => weight.checked_sub(p * weight).unwrap_or(Zero::zero()),
		}
	}

	/// consumes self and returns the combination of `self` and `rhs`, taking the sign into account.
	pub fn sum(self, rhs: Self) -> Self {
		match (self, rhs) {
			(FeeMultiplier::Positive(p1, r1), FeeMultiplier::Positive(p2, r2)) => {
				// because the add implementation silently saturates. A perbill should saturate but
				// once we have a proper `Float` type this can be improved.
				let billion = 1_000_000_000;
				if p1.0 + p2.0 > billion {
					FeeMultiplier::Positive(
						Perbill::from_parts((p1.0 + p2.0) % billion),
						r1.saturating_add(r2).saturating_add(1)
					)
				} else {
					FeeMultiplier::Positive(p1 + p2, r1.saturating_add(r2))
				}
			},
			(FeeMultiplier::Negative(p1), FeeMultiplier::Negative(p2)) => {
				// the sum impl of perbill simply caps this. This cannot grow more than -1.
				FeeMultiplier::Negative(p1 + p2)
			},
			(FeeMultiplier::Positive(p1, r1), FeeMultiplier::Negative(p2)) => {
				if let Some(new_p) = p1.0.checked_sub(p2.0) {
					FeeMultiplier::Positive(Perbill::from_parts(new_p), r1)
				} else {
					let new_p = Perbill::from_parts(p2.0 - p1.0);
					if r1 > 0 {
						FeeMultiplier::Positive(new_p, r1-1)
					} else {
						FeeMultiplier::Negative(new_p)
					}
				}
			},
			(FeeMultiplier::Negative(_), FeeMultiplier::Positive(_, _)) => {
				rhs.sum(self)
			},
		}
	}
}

impl Default for FeeMultiplier {
	fn default() -> Self {
		FeeMultiplier::Positive(Perbill::zero(), Zero::zero())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn p(percent: u32) -> Perbill {
		Perbill::from_parts(percent * 1_000_000_0)
	}

	#[test]
	fn fee_multiplier_can_sum() {
		assert_eq!(
			FeeMultiplier::Positive(p(10), 1).sum(FeeMultiplier::Positive(p(10), 1)),
			FeeMultiplier::Positive(p(20), 2)
		);

		assert_eq!(
			FeeMultiplier::Positive(p(60), 0).sum(FeeMultiplier::Positive(p(60), 1)),
			FeeMultiplier::Positive(p(20), 2)
		);

		assert_eq!(
			FeeMultiplier::Positive(p(60), 0).sum(FeeMultiplier::Positive(p(60), 0)),
			FeeMultiplier::Positive(p(20), 1)
		);

		assert_eq!(
			FeeMultiplier::Positive(p(10), 0).sum(FeeMultiplier::Positive(p(10), 1)),
			FeeMultiplier::Positive(p(20), 1)
		);

		assert_eq!(
			FeeMultiplier::Positive(p(10), 0).sum(FeeMultiplier::Positive(p(10), 1)),
			FeeMultiplier::Positive(p(20), 1)
		);

		assert_eq!(
			FeeMultiplier::Positive(p(10), 0).sum(FeeMultiplier::Negative(p(10))),
			FeeMultiplier::Positive(p(0), 0)
		);

		// zero
		assert_eq!(
			FeeMultiplier::Positive(p(0), 0).sum(FeeMultiplier::Negative(p(10))),
			FeeMultiplier::Negative(p(10))
		);
		assert_eq!(
			FeeMultiplier::Negative(p(0)).sum(FeeMultiplier::Positive(p(10), 2)),
			FeeMultiplier::Positive(p(10), 2)
		);

		// asymmetric operation.
		assert_eq!(
			FeeMultiplier::Positive(p(10), 0).sum(FeeMultiplier::Negative(p(30))),
			FeeMultiplier::Negative(p(20))
		);
		assert_eq!(
			FeeMultiplier::Negative(p(30)).sum(FeeMultiplier::Positive(p(10), 0)),
			FeeMultiplier::Negative(p(20))
		);
	}
}