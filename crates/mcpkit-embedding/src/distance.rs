//! Distance and similarity metrics for vector operations.

use serde::{Deserialize, Serialize};

/// Distance metric for comparing embedding vectors.
///
/// Different metrics are suited for different use cases:
/// - **Cosine**: Best for semantic similarity, direction-based comparison
/// - **Euclidean**: Best for absolute distance, clustering applications
/// - **`DotProduct`**: Fast alternative for normalized vectors (equivalent to cosine)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Cosine similarity (1 - `cosine_distance`).
    ///
    /// Returns values between -1 and 1 where:
    /// - 1.0: identical direction (most similar)
    /// - 0.0: orthogonal (unrelated)
    /// - -1.0: opposite direction (most dissimilar)
    ///
    /// Best for semantic similarity where magnitude doesn't matter.
    #[default]
    Cosine,

    /// Euclidean distance (L2 norm).
    ///
    /// Returns values >= 0 where:
    /// - 0.0: identical vectors (most similar)
    /// - Higher values: more dissimilar
    ///
    /// Best when absolute differences matter.
    Euclidean,

    /// Dot product (inner product).
    ///
    /// For normalized vectors, equivalent to cosine similarity
    /// but faster to compute.
    ///
    /// Higher values indicate more similarity.
    DotProduct,
}

impl DistanceMetric {
    /// Calculate the distance/similarity between two vectors.
    ///
    /// Returns `None` if vectors have different lengths or are invalid
    /// (e.g., zero magnitude for cosine).
    #[must_use]
    pub fn calculate(&self, a: &[f32], b: &[f32]) -> Option<f32> {
        if a.len() != b.len() {
            return None;
        }

        match self {
            Self::Cosine => cosine_similarity(a, b),
            Self::Euclidean => Some(euclidean_distance(a, b)),
            Self::DotProduct => Some(dot_product(a, b)),
        }
    }

    /// Whether higher values indicate greater similarity.
    ///
    /// - Cosine: true (1.0 = most similar)
    /// - `DotProduct`: true (higher = more similar)
    /// - Euclidean: false (0.0 = most similar)
    #[must_use]
    pub const fn higher_is_better(&self) -> bool {
        match self {
            Self::Cosine | Self::DotProduct => true,
            Self::Euclidean => false,
        }
    }
}

/// Compute cosine similarity between two vectors.
///
/// Returns `None` if either vector has zero magnitude.
#[must_use]
fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return None;
    }

    Some(dot / (mag_a * mag_b))
}

/// Compute Euclidean distance (L2 norm) between two vectors.
#[must_use]
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Compute dot product between two vectors.
#[must_use]
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let v = vec![1.0, 0.0, 0.0];
        let score = DistanceMetric::Cosine.calculate(&v, &v).unwrap();
        assert!((score - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let score = DistanceMetric::Cosine.calculate(&a, &b).unwrap();
        assert!(score.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let score = DistanceMetric::Cosine.calculate(&a, &b).unwrap();
        assert!((score - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let dist = DistanceMetric::Euclidean.calculate(&v, &v).unwrap();
        assert!(dist.abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_known_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let dist = DistanceMetric::Euclidean.calculate(&a, &b).unwrap();
        assert!((dist - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let dot = DistanceMetric::DotProduct.calculate(&a, &b).unwrap();
        // 1*4 + 2*5 + 3*6 = 32
        assert!((dot - 32.0).abs() < 0.0001);
    }

    #[test]
    fn test_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(DistanceMetric::Cosine.calculate(&a, &b).is_none());
    }

    #[test]
    fn test_higher_is_better() {
        assert!(DistanceMetric::Cosine.higher_is_better());
        assert!(DistanceMetric::DotProduct.higher_is_better());
        assert!(!DistanceMetric::Euclidean.higher_is_better());
    }
}
