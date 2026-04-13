use super::traits::Filter;
use crate::ml::MLFeatures;

/// Basic holder-based filter implementing the Filter trait
pub struct BasicFilter {
    min_holders: Option<u32>,
    max_holders: Option<u32>,
}

impl BasicFilter {
    /// Create new BasicFilter with holder thresholds
    pub fn new(min_holders: Option<u32>, max_holders: Option<u32>) -> Self {
        Self {
            min_holders,
            max_holders,
        }
    }
}

impl Filter for BasicFilter {
    fn apply(&self, features: &MLFeatures) -> bool {
        let holders = features.num_holders;
        if let Some(min_holders) = self.min_holders {
            if holders < min_holders {
                return false;
            }
        }
        if let Some(max_holders) = self.max_holders {
            if holders > max_holders {
                return false;
            }
        }
        true
    }
}
