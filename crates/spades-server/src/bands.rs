//! Rating-band segmentation for matchmaking. A seeker's Glicko rating maps to
//! one of three broad skill tiers; the matchmaker only pairs seeks within the
//! same band (see `matchmaking::try_match`).

/// Upper-exclusive boundaries between the three bands, in the visible
/// 1500-scale. `< 1400` = band 0 (Low), `1400..1600` = band 1 (Mid),
/// `>= 1600` = band 2 (High). Server config — tune without a protocol change.
pub const BAND_BOUNDARIES: [f64; 2] = [1400.0, 1600.0];

/// Map a rating to its band index (0 = Low, 1 = Mid, 2 = High).
pub fn band_of(rating: f64) -> u8 {
    BAND_BOUNDARIES.iter().filter(|&&b| rating >= b).count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boundaries_map_to_expected_tiers() {
        assert_eq!(band_of(0.0), 0);
        assert_eq!(band_of(1399.9), 0);
        assert_eq!(band_of(1400.0), 1); // lower boundary -> Mid
        assert_eq!(band_of(1500.0), 1);
        assert_eq!(band_of(1599.9), 1);
        assert_eq!(band_of(1600.0), 2); // upper boundary -> High
        assert_eq!(band_of(2000.0), 2);
    }
}
