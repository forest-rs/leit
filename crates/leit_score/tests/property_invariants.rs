//! Property-based invariant tests for `leit_score`.

use leit_core::FieldId;
use leit_score::{Bm25FScorer, Bm25Scorer, FieldStats, ScoringStats};
use proptest::collection::vec;
use proptest::prelude::*;

proptest! {
    #[test]
    fn bm25_returns_finite_scores_for_valid_stats(
        term_frequency in 1u32..100u32,
        doc_length in 1u32..5_000u32,
        avg_doc_length in 0.1f32..5_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25Scorer::new();
        let score = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });

        prop_assert!(score.as_f32().is_finite());
    }

    #[test]
    fn bm25_returns_zero_for_invalid_collection_stats(
        term_frequency in 0u32..10u32,
        doc_length in 0u32..1_000u32,
        doc_count in 0u32..100u32,
        doc_frequency in 0u32..100u32,
        avg_doc_length in prop_oneof![Just(0.0f32), Just(f32::INFINITY), Just(f32::NEG_INFINITY), Just(f32::NAN)],
    ) {
        let scorer = Bm25Scorer::new();
        let score = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });

        prop_assert_eq!(score, leit_core::Score::ZERO);
    }

    #[test]
    fn bm25f_returns_finite_scores_for_valid_field_stats(
        fields in vec((1u32..50u32, 1u32..500u32, 0.1f32..5.0f32), 1..6),
        avg_doc_length in 0.1f32..2_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let field_stats: Vec<_> = fields
            .into_iter()
            .enumerate()
            .map(|(idx, (term_frequency, field_length, weight))| FieldStats {
                field_id: FieldId::new(
                    u32::try_from(idx).expect("generated field index should fit within u32"),
                ),
                term_frequency,
                field_length,
                weight,
            })
            .collect();

        let scorer = Bm25FScorer::new();
        let score = scorer.score(&field_stats, avg_doc_length, doc_count, doc_frequency);

        prop_assert!(score.as_f32().is_finite());
    }

    #[test]
    fn bm25_score_is_monotonic_in_term_frequency(
        term_frequency in 1u32..100u32,
        extra_frequency in 1u32..100u32,
        doc_length in 1u32..5_000u32,
        avg_doc_length in 0.1f32..5_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25Scorer::new();
        let low = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });
        let high = scorer.score(&ScoringStats {
            term_frequency: term_frequency.saturating_add(extra_frequency),
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });

        prop_assert!(high >= low);
    }

    #[test]
    fn bm25_score_is_antitonic_in_document_length(
        term_frequency in 1u32..100u32,
        doc_length in 1u32..5_000u32,
        extra_length in 1u32..5_000u32,
        avg_doc_length in 0.1f32..5_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25Scorer::new();
        let short = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });
        let long = scorer.score(&ScoringStats {
            term_frequency,
            doc_length: doc_length.saturating_add(extra_length),
            avg_doc_length,
            doc_count,
            doc_frequency,
        });

        prop_assert!(short >= long);
    }

    #[test]
    fn bm25_score_is_antitonic_in_document_frequency(
        term_frequency in 1u32..100u32,
        doc_length in 1u32..5_000u32,
        avg_doc_length in 0.1f32..5_000.0f32,
        doc_count in 2u32..100_000u32,
        doc_frequency in 1u32..99_999u32,
        extra_frequency in 1u32..1_000u32,
    ) {
        prop_assume!(doc_frequency < doc_count);
        let higher_df = doc_frequency.saturating_add(extra_frequency).min(doc_count);
        prop_assume!(higher_df >= doc_frequency);

        let scorer = Bm25Scorer::new();
        let rare = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        });
        let common = scorer.score(&ScoringStats {
            term_frequency,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency: higher_df,
        });

        prop_assert!(rare >= common);
    }

    #[test]
    fn bm25f_score_is_monotonic_in_term_frequency(
        term_frequency in 1u32..50u32,
        extra_frequency in 1u32..50u32,
        field_length in 1u32..500u32,
        weight in 0.1f32..5.0f32,
        avg_doc_length in 0.1f32..2_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25FScorer::new();
        let low_fields = [FieldStats {
            field_id: FieldId::new(0),
            term_frequency,
            field_length,
            weight,
        }];
        let high_fields = [FieldStats {
            field_id: FieldId::new(0),
            term_frequency: term_frequency.saturating_add(extra_frequency),
            field_length,
            weight,
        }];

        let low = scorer.score(&low_fields, avg_doc_length, doc_count, doc_frequency);
        let high = scorer.score(&high_fields, avg_doc_length, doc_count, doc_frequency);

        prop_assert!(high >= low);
    }

    #[test]
    fn bm25f_score_is_antitonic_in_weighted_length(
        term_frequency in 1u32..50u32,
        field_length in 1u32..500u32,
        extra_length in 1u32..500u32,
        weight in 0.1f32..5.0f32,
        avg_doc_length in 0.1f32..2_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25FScorer::new();
        let short_fields = [FieldStats {
            field_id: FieldId::new(0),
            term_frequency,
            field_length,
            weight,
        }];
        let long_fields = [FieldStats {
            field_id: FieldId::new(0),
            term_frequency,
            field_length: field_length.saturating_add(extra_length),
            weight,
        }];

        let short = scorer.score(&short_fields, avg_doc_length, doc_count, doc_frequency);
        let long = scorer.score(&long_fields, avg_doc_length, doc_count, doc_frequency);

        prop_assert!(short >= long);
    }

    #[test]
    fn bm25f_ignores_zero_weight_fields(
        term_frequency in 1u32..50u32,
        field_length in 1u32..500u32,
        avg_doc_length in 0.1f32..2_000.0f32,
        doc_count in 1u32..100_000u32,
        doc_frequency in 1u32..100_000u32,
    ) {
        prop_assume!(doc_frequency <= doc_count);

        let scorer = Bm25FScorer::new();
        let base_fields = [FieldStats {
            field_id: FieldId::new(0),
            term_frequency,
            field_length,
            weight: 1.0,
        }];
        let extended_fields = [
            FieldStats {
                field_id: FieldId::new(0),
                term_frequency,
                field_length,
                weight: 1.0,
            },
            FieldStats {
                field_id: FieldId::new(1),
                term_frequency: term_frequency.saturating_mul(2),
                field_length: field_length.saturating_mul(2),
                weight: 0.0,
            },
        ];

        let base = scorer.score(&base_fields, avg_doc_length, doc_count, doc_frequency);
        let extended = scorer.score(&extended_fields, avg_doc_length, doc_count, doc_frequency);

        prop_assert_eq!(base, extended);
    }
}
