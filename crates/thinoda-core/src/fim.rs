//! FIM domain primitives shared by the map, structure view and format adapters.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A containment usage's multiplicity, retaining its original decimal notation.
///
/// Bounds are decimal strings rather than machine integers: the exchange schema
/// does not limit multiplicities to Rust's integer range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Cardinality {
    source: String,
    separator: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardinalityError {
    #[error("FIM cardinality must contain decimal minimum:maximum or minimum:*")]
    Syntax,
    #[error("FIM cardinality minimum exceeds its maximum")]
    Reversed,
    #[error("FIM cardinality exceeds the supported 1024-byte limit")]
    TooLong,
}

impl Cardinality {
    pub fn parse(source: &str) -> Result<Self, CardinalityError> {
        if source.len() > 1024 {
            return Err(CardinalityError::TooLong);
        }
        let (minimum, maximum) = source.split_once(':').ok_or(CardinalityError::Syntax)?;
        let decimal = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
        if !decimal(minimum) || (maximum != "*" && !decimal(maximum)) {
            return Err(CardinalityError::Syntax);
        }
        if maximum != "*" {
            let min = minimum.trim_start_matches('0');
            let max = maximum.trim_start_matches('0');
            if (min.len(), min) > (max.len(), max) {
                return Err(CardinalityError::Reversed);
            }
        }
        Ok(Self {
            source: source.to_owned(),
            separator: minimum.len(),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn minimum(&self) -> &str {
        &self.source[..self.separator]
    }

    /// `None` represents unbounded, never a silently chosen finite limit.
    pub fn maximum(&self) -> Option<&str> {
        let maximum = &self.source[self.separator + 1..];
        (maximum != "*").then_some(maximum)
    }

    pub fn is_optional(&self) -> bool {
        self.minimum().bytes().all(|b| b == b'0')
    }
}

impl TryFrom<String> for Cardinality {
    type Error = CardinalityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Cardinality> for String {
    fn from(value: Cardinality) -> Self {
        value.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_lexical_bounds_and_unboundedness() {
        let cardinality = Cardinality::parse("000:00012").unwrap();
        assert_eq!(cardinality.source(), "000:00012");
        assert_eq!(cardinality.minimum(), "000");
        assert_eq!(cardinality.maximum(), Some("00012"));
        assert!(cardinality.is_optional());
        let cardinality = Cardinality::parse("1:*").unwrap();
        assert_eq!(cardinality.maximum(), None);
        assert!(!cardinality.is_optional());
    }

    #[test]
    fn supports_bounds_beyond_machine_integer_range() {
        assert!(Cardinality::parse("18446744073709551616:18446744073709551617").is_ok());
        assert_eq!(
            Cardinality::parse("18446744073709551617:18446744073709551616"),
            Err(CardinalityError::Reversed)
        );
        assert!(Cardinality::parse("0000:0").is_ok());
        assert!(Cardinality::parse("0001:01").is_ok());
    }

    #[test]
    fn rejects_invalid_and_reversed_ranges() {
        for value in [
            "", "1", ":1", "1:", "*:*", "-1:2", "1:2:3", " 1:1", "1:1 ", "١:٢",
        ] {
            assert_eq!(
                Cardinality::parse(value),
                Err(CardinalityError::Syntax),
                "{value}"
            );
        }
        assert_eq!(Cardinality::parse("2:1"), Err(CardinalityError::Reversed));
        assert_eq!(
            Cardinality::parse(&format!("{}:*", "1".repeat(1024))),
            Err(CardinalityError::TooLong)
        );
    }

    #[test]
    fn native_deserialization_cannot_bypass_range_checks() {
        let value = Cardinality::parse("0:*").unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"0:*\"");
        assert_eq!(
            serde_json::from_str::<Cardinality>("\"0:*\"").unwrap(),
            value
        );
        assert!(serde_json::from_str::<Cardinality>("\"2:1\"").is_err());
    }
}
