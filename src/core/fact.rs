//! Fact - Core data structure
//!
//! A fact is the fundamental unit of knowledge in meh.
//!
//! # Schema
//! See `../../plan/DECISIONS_UNIFIED.md` section 9 for schema.
//!
//! # Key Properties
//! - **id**: ULID (sortable, unique)
//! - **path**: Hierarchical location (e.g., @products/alpha/api/timeout)
//! - **content**: Markdown content
//! - **trust_score**: 0.0-1.0 Bayesian trust
//! - **supersedes**: For append-only corrections

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use super::trust::TrustCalculator;

/// Source type for facts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Local storage (your machine)
    Local,
    /// Company server
    Company,
    /// Global public (memoraihub.io)
    Global,
    /// Third-party packages (npm, etc.)
    Npm,
}

impl Default for Source {
    fn default() -> Self {
        Source::Local
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Local => write!(f, "local"),
            Source::Company => write!(f, "company"),
            Source::Global => write!(f, "global"),
            Source::Npm => write!(f, "npm"),
        }
    }
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Source::Local),
            "company" => Ok(Source::Company),
            "global" => Ok(Source::Global),
            "npm" => Ok(Source::Npm),
            _ => anyhow::bail!("Unknown source: {}", s),
        }
    }
}

/// Fact status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Active and current
    #[default]
    Active,
    /// Superseded by another fact
    Superseded,
    /// Marked as deprecated
    Deprecated,
    /// Archived (old, but kept)
    Archived,
    /// Pending human review (for "ask" write policy)
    PendingReview,
}

/// Author type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthorType {
    Human,
    Ai,
    System,
}

impl Default for AuthorType {
    fn default() -> Self {
        AuthorType::Ai
    }
}

/// Fact type (for append-only operations)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FactType {
    /// Original fact
    #[default]
    Fact,
    /// Correction of another fact
    Correction,
    /// Extension of another fact
    Extension,
    /// Warning/caveat
    Warning,
    /// Deprecation notice
    Deprecation,
}

/// A fact - the fundamental unit of knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    /// Unique identifier (ULID)
    pub id: Ulid,

    /// Hierarchical path (e.g., @products/alpha/api/timeout)
    pub path: String,

    /// Title (short description)
    pub title: String,

    /// Full content (Markdown)
    pub content: String,

    /// Summary (1-3 sentences, auto-generated or manual)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Tags for cross-cutting concerns
    #[serde(default)]
    pub tags: Vec<String>,

    /// Source type
    #[serde(default)]
    pub source: Source,

    /// Namespace (for multi-tenant)
    #[serde(default)]
    pub namespace: String,

    /// Trust score (0.0-1.0)
    #[serde(default = "default_trust")]
    pub trust_score: f32,

    /// Status
    #[serde(default)]
    pub status: Status,

    /// Fact type
    #[serde(default)]
    pub fact_type: FactType,

    /// ID of fact this supersedes (for corrections)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Ulid>,

    /// IDs of facts this extends
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<Ulid>,

    /// Author type
    #[serde(default)]
    pub author_type: AuthorType,

    /// Author identifier
    #[serde(default)]
    pub author_id: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Last access timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_at: Option<DateTime<Utc>>,
}

fn default_trust() -> f32 {
    0.5
}

impl Fact {
    /// Create a new fact with calculated initial trust
    pub fn new(
        path: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let calc = TrustCalculator::new();
        let initial_trust = calc.initial_trust(AuthorType::default(), Source::default());

        Self {
            id: Ulid::new(),
            path: path.into(),
            title: title.into(),
            content: content.into(),
            summary: None,
            tags: Vec::new(),
            source: Source::default(),
            namespace: String::new(),
            trust_score: initial_trust,
            status: Status::default(),
            fact_type: FactType::default(),
            supersedes: None,
            extends: Vec::new(),
            author_type: AuthorType::default(),
            author_id: String::new(),
            created_at: now,
            updated_at: now,
            accessed_at: None,
        }
    }

    /// Create a new fact with specific author (calculates trust based on author type)
    pub fn new_with_author(
        path: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        author_type: AuthorType,
        author_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let calc = TrustCalculator::new();
        let initial_trust = calc.initial_trust(author_type, Source::default());

        Self {
            id: Ulid::new(),
            path: path.into(),
            title: title.into(),
            content: content.into(),
            summary: None,
            tags: Vec::new(),
            source: Source::default(),
            namespace: String::new(),
            trust_score: initial_trust,
            status: Status::default(),
            fact_type: FactType::default(),
            supersedes: None,
            extends: Vec::new(),
            author_type,
            author_id: author_id.into(),
            created_at: now,
            updated_at: now,
            accessed_at: None,
        }
    }

    /// Recalculate trust score based on current state
    pub fn recalculate_trust(&mut self, confirmation_count: u32) {
        let calc = TrustCalculator::new();
        let base_trust = calc.initial_trust(self.author_type, self.source);
        self.trust_score = calc.effective_trust(
            base_trust,
            self.created_at,
            self.status,
            self.fact_type,
            confirmation_count,
        );
    }

    /// Create a correction of another fact
    pub fn correction(original: &Fact, new_content: impl Into<String>) -> Self {
        let mut fact = Fact::new(&original.path, &original.title, new_content);
        fact.supersedes = Some(original.id);
        fact.fact_type = FactType::Correction;
        fact.tags = original.tags.clone();
        fact
    }

    /// Create an extension of another fact
    pub fn extension(original: &Fact, additional_content: impl Into<String>) -> Self {
        let mut fact = Fact::new(
            &original.path,
            format!("{} (extension)", original.title),
            additional_content,
        );
        fact.extends = vec![original.id];
        fact.fact_type = FactType::Extension;
        fact
    }

    /// Set tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set source
    pub fn with_source(mut self, source: Source) -> Self {
        self.source = source;
        self
    }

    /// Set author
    pub fn with_author(mut self, author_type: AuthorType, author_id: impl Into<String>) -> Self {
        self.author_type = author_type;
        self.author_id = author_id.into();
        self
    }

    /// Generate summary from content (first sentence or first N chars)
    pub fn generate_summary(&mut self, max_chars: usize) {
        let content = self.content.trim();

        // Try to find first sentence
        if let Some(end) = content.find(|c| c == '.' || c == '!' || c == '?') {
            if end < max_chars {
                self.summary = Some(content[..=end].to_string());
                return;
            }
        }

        // Fallback to first N chars
        if content.len() <= max_chars {
            self.summary = Some(content.to_string());
        } else {
            let truncated = &content[..max_chars];
            // Try to break at word boundary
            if let Some(last_space) = truncated.rfind(' ') {
                self.summary = Some(format!("{}...", &truncated[..last_space]));
            } else {
                self.summary = Some(format!("{}...", truncated));
            }
        }
    }

    /// Get short ID (first 8 chars)
    pub fn short_id(&self) -> String {
        self.id.to_string()[..8].to_lowercase()
    }

    /// Format as meh ID
    pub fn meh_id(&self) -> String {
        format!("meh-{}", self.short_id())
    }
}

impl std::fmt::Display for Fact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.meh_id(), self.path, self.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_fact() {
        let fact = Fact::new(
            "@products/alpha/api/timeout",
            "API Timeout",
            "API timeout is 30 seconds.",
        );

        assert!(!fact.id.to_string().is_empty());
        assert_eq!(fact.path, "@products/alpha/api/timeout");
        assert_eq!(fact.title, "API Timeout");
        assert_eq!(fact.trust_score, 0.5);
        assert_eq!(fact.status, Status::Active);
    }

    #[test]
    fn test_correction() {
        let original = Fact::new(
            "@products/alpha/api/timeout",
            "API Timeout",
            "API timeout is 30 seconds.",
        );

        let correction = Fact::correction(&original, "API timeout is 60 seconds.");

        assert_eq!(correction.supersedes, Some(original.id));
        assert_eq!(correction.fact_type, FactType::Correction);
        assert_eq!(correction.path, original.path);
    }

    #[test]
    fn test_generate_summary() {
        let mut fact = Fact::new(
            "@test",
            "Test",
            "This is the first sentence. This is the second.",
        );
        fact.generate_summary(100);
        assert_eq!(
            fact.summary,
            Some("This is the first sentence.".to_string())
        );
    }

    #[test]
    fn test_meh_id() {
        let fact = Fact::new("@test", "Test", "Content");
        let meh_id = fact.meh_id();
        assert!(meh_id.starts_with("meh-"));
        assert_eq!(meh_id.len(), 12); // "meh-" + 8 chars
    }

    // === Additional tests ===

    #[test]
    fn test_extension() {
        let original = Fact::new("@test/path", "Original", "Original content");
        let extension = Fact::extension(&original, "Additional info");

        assert_eq!(extension.extends, vec![original.id]);
        assert_eq!(extension.fact_type, FactType::Extension);
        assert!(extension.title.contains("extension"));
    }

    #[test]
    fn test_with_tags() {
        let fact = Fact::new("@test", "Test", "Content")
            .with_tags(vec!["bug".to_string(), "critical".to_string()]);

        assert_eq!(fact.tags.len(), 2);
        assert!(fact.tags.contains(&"bug".to_string()));
    }

    #[test]
    fn test_with_source() {
        let fact = Fact::new("@test", "Test", "Content").with_source(Source::Company);

        assert_eq!(fact.source, Source::Company);
    }

    #[test]
    fn test_with_author() {
        let fact = Fact::new("@test", "Test", "Content").with_author(AuthorType::Human, "user123");

        assert_eq!(fact.author_type, AuthorType::Human);
        assert_eq!(fact.author_id, "user123");
    }

    #[test]
    fn test_new_with_author_trust() {
        let ai_fact = Fact::new_with_author("@test", "AI Fact", "Content", AuthorType::Ai, "gpt-4");
        assert_eq!(ai_fact.trust_score, 0.5); // AI default

        let human_fact =
            Fact::new_with_author("@test", "Human Fact", "Content", AuthorType::Human, "john");
        assert_eq!(human_fact.trust_score, 0.8); // Human default
    }

    #[test]
    fn test_source_from_str() {
        assert_eq!("local".parse::<Source>().unwrap(), Source::Local);
        assert_eq!("company".parse::<Source>().unwrap(), Source::Company);
        assert_eq!("global".parse::<Source>().unwrap(), Source::Global);
        assert_eq!("npm".parse::<Source>().unwrap(), Source::Npm);
        assert!("unknown".parse::<Source>().is_err());
    }

    #[test]
    fn test_source_display() {
        assert_eq!(format!("{}", Source::Local), "local");
        assert_eq!(format!("{}", Source::Company), "company");
    }

    #[test]
    fn test_generate_summary_long_content() {
        let long_content = "This is a very long piece of content that exceeds the maximum character limit and should be truncated at a word boundary to avoid cutting words in the middle.";
        let mut fact = Fact::new("@test", "Test", long_content);
        fact.generate_summary(50);

        let summary = fact.summary.unwrap();
        assert!(summary.len() <= 53); // 50 + "..."
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_generate_summary_short_content() {
        let mut fact = Fact::new("@test", "Test", "Short.");
        fact.generate_summary(100);
        assert_eq!(fact.summary, Some("Short.".to_string()));
    }

    #[test]
    fn test_short_id() {
        let fact = Fact::new("@test", "Test", "Content");
        let short = fact.short_id();
        assert_eq!(short.len(), 8);
        // Should be lowercase
        assert_eq!(short, short.to_lowercase());
    }

    #[test]
    fn test_display() {
        let fact = Fact::new("@test/path", "My Title", "Content");
        let display = format!("{}", fact);
        assert!(display.contains("meh-"));
        assert!(display.contains("@test/path"));
        assert!(display.contains("My Title"));
    }

    #[test]
    fn test_status_default() {
        let fact = Fact::new("@test", "Test", "Content");
        assert_eq!(fact.status, Status::Active);
    }

    #[test]
    fn test_fact_type_default() {
        let fact = Fact::new("@test", "Test", "Content");
        assert_eq!(fact.fact_type, FactType::Fact);
    }

    #[test]
    fn test_timestamps() {
        let before = Utc::now();
        let fact = Fact::new("@test", "Test", "Content");
        let after = Utc::now();

        assert!(fact.created_at >= before);
        assert!(fact.created_at <= after);
        assert_eq!(fact.created_at, fact.updated_at);
        assert!(fact.accessed_at.is_none());
    }

    #[test]
    fn test_correction_preserves_tags() {
        let original =
            Fact::new("@test", "Test", "Content").with_tags(vec!["important".to_string()]);

        let correction = Fact::correction(&original, "New content");
        assert_eq!(correction.tags, vec!["important".to_string()]);
    }

    #[test]
    fn test_unique_ids() {
        let fact1 = Fact::new("@test", "Test1", "Content1");
        let fact2 = Fact::new("@test", "Test2", "Content2");
        assert_ne!(fact1.id, fact2.id);
    }
}
