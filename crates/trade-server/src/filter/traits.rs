use crate::ml::MLFeatures;

pub trait Filter {
    /// Apply filtering logic to ML features
    /// Returns true if the signal should be kept, false if filtered out
    fn apply(&self, features: &MLFeatures) -> bool;
}
