// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Zipfian vocabulary distribution over 500 common English words.

/// 500 most common English words for vocabulary.
#[rustfmt::skip]
const VOCABULARY: &[&str] = &[
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i",
    "it", "for", "not", "on", "with", "he", "as", "you", "do", "at",
    "this", "but", "his", "by", "from", "they", "we", "say", "her", "she",
    "or", "an", "will", "my", "one", "all", "would", "there", "their", "what",
    "so", "up", "out", "if", "about", "who", "get", "which", "go", "me",
    "when", "make", "can", "like", "time", "no", "just", "him", "know", "take",
    "people", "into", "year", "your", "good", "some", "could", "them", "see", "other",
    "than", "then", "now", "look", "only", "come", "its", "over", "think", "also",
    "back", "after", "use", "two", "how", "our", "work", "first", "well", "way",
    "even", "new", "want", "because", "any", "these", "give", "day", "most", "us",
    "is", "was", "are", "been", "being", "having", "did", "does", "doing", "shall",
    "should", "may", "might", "must", "ought", "let", "help", "try", "start", "begin",
    "stop", "seem", "appear", "become", "grow", "turn", "feel", "sound", "taste", "smell",
    "hear", "watch", "observe", "notice", "find", "discover", "learn", "understand", "believe", "suppose",
    "assume", "guess", "tell", "speak", "talk", "ask", "answer", "reply", "respond", "shout",
    "cry", "sing", "play", "dance", "jump", "run", "walk", "move", "stand", "sit",
    "lie", "eat", "drink", "sleep", "wake", "dream", "live", "die", "exist", "happen",
    "occur", "stay", "remain", "leave", "arrive", "depart", "travel", "ride", "drive", "fly",
    "sail", "swim", "climb", "fall", "rise", "sink", "float", "carry", "bring", "put",
    "place", "set", "lay", "hang", "hold", "keep", "throw", "catch", "push", "pull",
    "break", "fix", "build", "destroy", "create", "perform", "show", "hide", "open", "close",
    "lock", "unlock", "twist", "bend", "stretch", "squeeze", "press", "touch", "rub", "scratch",
    "strike", "hit", "beat", "kick", "punch", "slap", "grab", "hug", "kiss", "bite",
    "cut", "tear", "rip", "burn", "freeze", "heat", "cool", "warm", "melt", "boil",
    "cook", "fry", "bake", "roast", "toast", "grill", "broil", "steam", "poach", "simmer",
    "stew", "braise", "marinate", "blend", "mix", "stir", "whip", "knead", "fold", "roll",
    "shape", "form", "mold", "carve", "slice", "dice", "chop", "mince", "grate", "shred",
    "peel", "pare", "trim", "clean", "wash", "rinse", "drain", "dry", "wipe", "polish",
    "shine", "gleam", "glitter", "sparkle", "glow", "illuminate", "light", "brighten", "darken", "fade",
    "pale", "blanch", "blush", "redden", "whiten", "blacken", "color", "dye", "paint", "stain",
    "mark", "spot", "speck", "dot", "dash", "streak", "stripe", "patch", "part", "section",
    "segment", "division", "component", "element", "portion", "fraction", "fragment", "bit", "chunk", "block",
    "lump", "mass", "heap", "pile", "stack", "bundle", "cluster", "group", "set", "collection",
    "series", "sequence", "chain", "line", "row", "column", "table", "list", "array", "grid",
    "matrix", "network", "web", "system", "structure", "framework", "skeleton", "outline", "shape", "form",
    "figure", "image", "picture", "photo", "drawing", "sketch", "design", "pattern", "model", "example",
    "sample", "specimen", "instance", "case", "illustration", "demonstration", "proof", "evidence", "sign", "trace",
    "track", "footprint", "imprint", "impression", "print", "stamp", "seal", "signature", "autograph", "script",
    "handwriting", "text", "document", "paper", "sheet", "page", "leaf", "folio", "scroll", "roll",
    "volume", "book", "novel", "story", "tale", "narrative", "account", "description", "explanation", "interpretation",
    "analysis", "study", "research", "investigation", "examination", "inspection", "observation", "survey", "scan", "review",
    "check", "verification", "confirmation", "validation", "authentication", "authorization", "approval", "acceptance", "agreement", "consent",
    "permission", "allowance", "license", "privilege", "right", "claim", "demand", "request", "plea", "appeal",
    "petition", "proposal", "suggestion", "recommendation", "advice", "counsel", "guidance", "direction", "instruction", "lesson",
    "teaching", "training", "education", "knowledge", "wisdom", "understanding", "comprehension", "insight", "perception", "awareness",
    "consciousness", "attention", "focus", "concentration", "effort", "attempt", "endeavor", "struggle", "strive", "fight",
    "battle", "war", "conflict", "dispute", "argument", "disagreement", "difference", "contrast", "comparison", "similarity",
    "analogy", "metaphor", "simile", "symbol", "representation", "expression", "statement", "declaration", "assertion", "claim",
    "opinion", "view", "perspective", "standpoint", "position", "stance", "attitude", "mood", "emotion", "feeling",
    "sensation", "sense", "impression", "thought", "idea", "concept", "notion", "belief", "faith", "trust",
];

/// Zipfian vocabulary with s=1.0 over 500 common English words.
///
/// Provides deterministic term selection via binary search over a precomputed
/// cumulative distribution function.
#[derive(Clone, Debug)]
pub struct Vocabulary {
    cdf: Vec<f64>,
}

impl Vocabulary {
    /// Create a new Zipfian vocabulary with s=1.0.
    #[must_use]
    pub fn new() -> Self {
        // Precompute Zipfian CDF for s=1.0
        let mut harmonic_sum = 0.0;
        for i in 1..=VOCABULARY.len() {
            harmonic_sum += 1.0 / (i as f64);
        }

        let mut cdf = Vec::with_capacity(VOCABULARY.len());
        let mut cumulative = 0.0;
        for i in 1..=VOCABULARY.len() {
            cumulative += (1.0 / (i as f64)) / harmonic_sum;
            cdf.push(cumulative);
        }

        Self { cdf }
    }

    /// Get the term for the given normalized hash value [0, 1).
    ///
    /// Uses binary search over the Zipfian CDF to select a vocabulary index.
    pub fn term_for_hash(&self, normalized_hash: f64) -> &'static str {
        let idx = self
            .cdf
            .binary_search_by(|&probe| probe.partial_cmp(&normalized_hash).unwrap())
            .unwrap_or_else(|idx| idx)
            .min(VOCABULARY.len() - 1);
        VOCABULARY[idx]
    }

    /// Get the cumulative probability of the given vocabulary index.
    pub fn cumulative_probability(&self, index: usize) -> f64 {
        self.cdf.get(index).copied().unwrap_or(1.0)
    }

    /// Total vocabulary size.
    pub const fn vocabulary_size() -> usize {
        500
    }
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zipfian_distribution_sanity() {
        let vocab = Vocabulary::new();
        assert_eq!(vocab.cdf.len(), VOCABULARY.len());
        assert!(vocab.cdf[vocab.cdf.len() - 1] >= 0.99);
    }

    #[test]
    fn test_frequent_terms_more_common() {
        let vocab = Vocabulary::new();

        // Frequent term indices should have higher cumulative probability
        let freq_idx = 0; // "the" is first
        let rare_idx = VOCABULARY.len() - 1; // Last term

        let freq_prob = vocab.cumulative_probability(freq_idx);
        let rare_prob = vocab.cumulative_probability(rare_idx);

        // The rare term should be accessed at higher cumulative probability
        assert!(rare_prob > freq_prob);
    }

    #[test]
    fn test_term_for_hash_returns_valid_term() {
        let vocab = Vocabulary::new();
        let term1 = vocab.term_for_hash(0.1);
        let term2 = vocab.term_for_hash(0.5);
        let term3 = vocab.term_for_hash(0.9);

        assert!(!term1.is_empty());
        assert!(!term2.is_empty());
        assert!(!term3.is_empty());
    }

    #[test]
    fn test_vocabulary_size() {
        assert_eq!(VOCABULARY.len(), 500);
        assert_eq!(Vocabulary::vocabulary_size(), 500);
    }
}
