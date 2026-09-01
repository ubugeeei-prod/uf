use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalLiteContract {
    pub calendar: bool,
    pub duration: bool,
    pub instant: bool,
    pub plain_date: bool,
    pub plain_time: bool,
    pub zoned_date_time: bool,
}

impl Default for TemporalLiteContract {
    fn default() -> Self {
        Self {
            calendar: false,
            duration: true,
            instant: true,
            plain_date: true,
            plain_time: true,
            zoned_date_time: true,
        }
    }
}

pub fn lite_contract() -> TemporalLiteContract {
    TemporalLiteContract::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_lite_keeps_core_time_types() {
        let contract = lite_contract();

        assert!(contract.duration);
        assert!(contract.instant);
        assert!(contract.plain_date);
        assert!(contract.plain_time);
        assert!(contract.zoned_date_time);
        assert!(!contract.calendar);
    }
}
