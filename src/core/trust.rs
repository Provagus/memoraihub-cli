//! Trust Scoring System
//!
//! Bayesian trust scoring for facts based on:
//! - Author type (human > AI > system)
//! - Age decay (older facts lose trust over time)
//! - Validation boosts (corrections, confirmations)
//! - Source reputation

use chrono::{DateTime, Utc};

use super::fact::{AuthorType, FactType, Source, Status};

/// Trust scoring configuration
#[derive(Debug, Clone)]
pub struct TrustConfig {
    /// Base trust for human authors
    pub human_base: f32,
    /// Base trust for AI authors
    pub ai_base: f32,
    /// Base trust for system authors
    pub system_base: f32,
    
    /// Trust multiplier for local source
    pub source_local: f32,
    /// Trust multiplier for company source
    pub source_company: f32,
    /// Trust multiplier for global source
    pub source_global: f32,
    /// Trust multiplier for npm source
    pub source_npm: f32,
    
    /// Days until trust starts decaying
    pub decay_start_days: i64,
    /// Decay rate per day after decay_start_days
    pub decay_rate: f32,
    /// Minimum trust after decay
    pub decay_floor: f32,
    
    /// Boost for each confirmation
    pub confirmation_boost: f32,
    /// Penalty for being superseded
    pub superseded_penalty: f32,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            // Author base trust
            human_base: 0.8,
            ai_base: 0.5,
            system_base: 0.6,
            
            // Source multipliers
            source_local: 1.0,
            source_company: 0.95,
            source_global: 0.7,
            source_npm: 0.6,
            
            // Decay settings
            decay_start_days: 90,   // 3 months
            decay_rate: 0.005,      // 0.5% per day
            decay_floor: 0.2,       // Never go below 20%
            
            // Boost/penalty
            confirmation_boost: 0.1,
            superseded_penalty: 0.3,
        }
    }
}

/// Trust calculator
pub struct TrustCalculator {
    config: TrustConfig,
}

impl TrustCalculator {
    pub fn new() -> Self {
        Self {
            config: TrustConfig::default(),
        }
    }

    pub fn with_config(config: TrustConfig) -> Self {
        Self { config }
    }

    /// Calculate initial trust for a new fact
    pub fn initial_trust(&self, author_type: AuthorType, source: Source) -> f32 {
        let base = match author_type {
            AuthorType::Human => self.config.human_base,
            AuthorType::Ai => self.config.ai_base,
            AuthorType::System => self.config.system_base,
        };

        let source_mult = match source {
            Source::Local => self.config.source_local,
            Source::Company => self.config.source_company,
            Source::Global => self.config.source_global,
            Source::Npm => self.config.source_npm,
        };

        (base * source_mult).min(1.0).max(0.0)
    }

    /// Calculate trust decay based on age
    pub fn apply_decay(&self, trust: f32, created_at: DateTime<Utc>) -> f32 {
        let age_days = (Utc::now() - created_at).num_days();
        
        if age_days <= self.config.decay_start_days {
            return trust;
        }

        let decay_days = age_days - self.config.decay_start_days;
        let decay = decay_days as f32 * self.config.decay_rate;
        
        (trust - decay).max(self.config.decay_floor)
    }

    /// Apply boost for confirmation (another fact references this positively)
    pub fn apply_confirmation_boost(&self, trust: f32) -> f32 {
        (trust + self.config.confirmation_boost).min(1.0)
    }

    /// Apply penalty for being superseded
    pub fn apply_superseded_penalty(&self, trust: f32) -> f32 {
        (trust - self.config.superseded_penalty).max(0.0)
    }

    /// Calculate effective trust considering all factors
    pub fn effective_trust(
        &self,
        base_trust: f32,
        created_at: DateTime<Utc>,
        status: Status,
        fact_type: FactType,
        confirmation_count: u32,
    ) -> f32 {
        let mut trust = base_trust;

        // Apply decay
        trust = self.apply_decay(trust, created_at);

        // Apply status modifiers
        match status {
            Status::Active => {}
            Status::Superseded => trust = self.apply_superseded_penalty(trust),
            Status::Deprecated => trust *= 0.5,
            Status::Archived => trust *= 0.3,
        }

        // Corrections inherit less trust initially
        if fact_type == FactType::Correction {
            trust *= 0.9;
        }

        // Apply confirmation boosts
        for _ in 0..confirmation_count {
            trust = self.apply_confirmation_boost(trust);
        }

        trust.min(1.0).max(0.0)
    }
}

impl Default for TrustCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_initial_trust_human_local() {
        let calc = TrustCalculator::new();
        let trust = calc.initial_trust(AuthorType::Human, Source::Local);
        assert!((trust - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_initial_trust_ai_global() {
        let calc = TrustCalculator::new();
        let trust = calc.initial_trust(AuthorType::Ai, Source::Global);
        assert!((trust - 0.35).abs() < 0.01); // 0.5 * 0.7
    }

    #[test]
    fn test_decay_within_grace_period() {
        let calc = TrustCalculator::new();
        let created = Utc::now() - Duration::days(30);
        let trust = calc.apply_decay(0.8, created);
        assert!((trust - 0.8).abs() < 0.01); // No decay yet
    }

    #[test]
    fn test_decay_after_grace_period() {
        let calc = TrustCalculator::new();
        let created = Utc::now() - Duration::days(190); // 100 days past grace
        let trust = calc.apply_decay(0.8, created);
        // 100 * 0.005 = 0.5 decay
        assert!((trust - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_decay_floor() {
        let calc = TrustCalculator::new();
        let created = Utc::now() - Duration::days(500); // Very old
        let trust = calc.apply_decay(0.8, created);
        assert!((trust - 0.2).abs() < 0.01); // Floor is 0.2
    }

    #[test]
    fn test_confirmation_boost() {
        let calc = TrustCalculator::new();
        let trust = calc.apply_confirmation_boost(0.5);
        assert!((trust - 0.6).abs() < 0.01);
    }
}
