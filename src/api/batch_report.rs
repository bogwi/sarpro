//! Batch processing report structure

/// Batch processing report
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchReport {
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
}
