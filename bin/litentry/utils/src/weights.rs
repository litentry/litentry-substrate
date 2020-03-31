#![cfg_attr(not(feature = "std"), no_std)]

// Transaction Weight Examples
// https://substrate.dev/rustdocs/master/sp_runtime/weights/index.html
use frame_support::{
	dispatch::{WeighData, PaysFee},
	weights::{ DispatchClass, Weight, ClassifyDispatch},
};

// A "scale" to weigh transactions. This scale can be used with any transactions that take a
// single argument of type u32. The ultimate weight of the transaction is the / product of the
// transaction parameter and the field of this struct.
pub struct Linear(u32);

// The actual weight calculation happens in the `impl WeighData` block
impl WeighData<(&u32,)> for Linear {
	fn weigh_data(&self, (x,): (&u32,)) -> Weight {

		// Use saturation so that an extremely large parameter value
		// Does not cause overflow.
		x.saturating_mul(self.0)
	}
}

// The PaysFee trait indicates whether fees should actually be charged from the caller. If not,
// the weights are still applied toward the block maximums.
impl<T> PaysFee<T> for Linear {
	fn pays_fee(&self, _: T) -> bool {
		true
	}
}

// Any struct that is used to weigh data must also implement ClassifyDispatchInfo. Here we classify
// the transaction as Normal (as opposed to operational.)
impl<T> ClassifyDispatch<T> for Linear {
	fn classify_dispatch(&self, _: T) -> DispatchClass {
		// Classify all calls as Normal (which is the default)
		Default::default()
	}
}

// Another scale to weight transactions. This one is more complex. / It computes weight according
// to the formula a*x^2 + b*y + c where / a, b, and c are fields in the struct, and x and y are
// transaction / parameters.
pub struct Quadratic(u32, u32, u32);

impl WeighData<(&u32, &u32)> for Quadratic {
	fn weigh_data(&self, (x, y): (&u32, &u32)) -> Weight {

		let ax2 = x.saturating_mul(*x).saturating_mul(self.0);
		let by = y.saturating_mul(self.1);
		let c = self.2;

		ax2.saturating_add(by).saturating_add(c)
	}
}

impl<T> ClassifyDispatch<T> for Quadratic {
	fn classify_dispatch(&self, _: T) -> DispatchClass {
		// Classify all calls as Normal (which is the default)
		Default::default()
	}
}

impl<T> PaysFee<T> for Quadratic {
	fn pays_fee(&self, _: T) -> bool {
		true
	}
}

// A final scale to weight transactions. This one weighs transactions where the first parameter
// is bool. If the bool is true, then the weight is linear in the second parameter. Otherwise
// the weight is constant.
pub struct Conditional(u32);

impl WeighData<(&bool, &u32)> for Conditional {
	fn weigh_data(&self, (switch, val): (&bool, &u32)) -> Weight {

		if *switch {
			val.saturating_mul(self.0)
		}
		else {
			self.0
		}
	}
}

impl<T> PaysFee<T> for Conditional {
	fn pays_fee(&self, _: T) -> bool {
		true
	}
}

impl<T> ClassifyDispatch<T> for Conditional {
	fn classify_dispatch(&self, _: T) -> DispatchClass {
		// Classify all calls as Normal (which is the default)
		Default::default()
	}
}