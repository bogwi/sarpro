//! Shared types and enums used across SARPRO.
//! Includes `Polarization`, `AutoscaleStrategy`, `InputFormat`, `OutputFormat`,
//! bit depths (`BitDepth`, `BitDepthArg`), and `ProcessingOperation`.
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum PolarizationOperation {
    Sum,
    Diff,
    Ratio,
    NDiff,
    LogRatio,
}

impl std::fmt::Display for PolarizationOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PolarizationOperation::Sum => "Sum",
            PolarizationOperation::Diff => "Diff",
            PolarizationOperation::Ratio => "Ratio",
            PolarizationOperation::NDiff => "NDiff",
            PolarizationOperation::LogRatio => "LogRatio",
        };
        write!(f, "{}", s)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Polarization {
    Vv,
    Vh,
    Hh,
    Hv,
    Multiband,
    OP(PolarizationOperation),
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum ProcessingOperation {
    SingleBand,
    MultibandVvVh,
    MultibandHhHv,
    PolarOp(PolarizationOperation),
}

impl std::fmt::Display for ProcessingOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingOperation::SingleBand => write!(f, "SingleBand"),
            ProcessingOperation::MultibandVvVh => write!(f, "MultibandVvVh"),
            ProcessingOperation::MultibandHhHv => write!(f, "MultibandHhHv"),
            ProcessingOperation::PolarOp(op) => write!(f, "PolarOp({})", op),
        }
    }
}

// Manual implementation for ValueEnum since we have non-unit variants
impl clap::ValueEnum for Polarization {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Polarization::Vv,
            Polarization::Vh,
            Polarization::Hh,
            Polarization::Hv,
            Polarization::Multiband,
            Polarization::OP(PolarizationOperation::Sum),
            Polarization::OP(PolarizationOperation::Diff),
            Polarization::OP(PolarizationOperation::Ratio),
            Polarization::OP(PolarizationOperation::NDiff),
            Polarization::OP(PolarizationOperation::LogRatio),
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Polarization::Vv => clap::builder::PossibleValue::new("vv"),
            Polarization::Vh => clap::builder::PossibleValue::new("vh"),
            Polarization::Hh => clap::builder::PossibleValue::new("hh"),
            Polarization::Hv => clap::builder::PossibleValue::new("hv"),
            Polarization::Multiband => clap::builder::PossibleValue::new("multiband"),
            Polarization::OP(PolarizationOperation::Sum) => {
                clap::builder::PossibleValue::new("sum")
            }
            Polarization::OP(PolarizationOperation::Diff) => {
                clap::builder::PossibleValue::new("diff")
            }
            Polarization::OP(PolarizationOperation::Ratio) => {
                clap::builder::PossibleValue::new("ratio")
            }
            Polarization::OP(PolarizationOperation::NDiff) => {
                clap::builder::PossibleValue::new("n-diff")
            }
            Polarization::OP(PolarizationOperation::LogRatio) => {
                clap::builder::PossibleValue::new("log-ratio")
            }
        })
    }
}

impl std::fmt::Display for Polarization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Polarization::Vv => write!(f, "Vv"),
            Polarization::Vh => write!(f, "Vh"),
            Polarization::Hh => write!(f, "Hh"),
            Polarization::Hv => write!(f, "Hv"),
            Polarization::Multiband => write!(f, "Multiband"),
            Polarization::OP(op) => write!(f, "{}", op),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum AutoscaleStrategy {
    Standard,
    Robust,
    Adaptive,
    Equalized,
    Tamed,
    Default,
}

impl std::fmt::Display for AutoscaleStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutoscaleStrategy::Standard => write!(f, "Standard"),
            AutoscaleStrategy::Robust => write!(f, "Robust"),
            AutoscaleStrategy::Adaptive => write!(f, "Adaptive"),
            AutoscaleStrategy::Equalized => write!(f, "Equalized"),
            AutoscaleStrategy::Tamed => write!(f, "Tamed"),
            AutoscaleStrategy::Default => write!(f, "Default"),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum InputFormat {
    Safe,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum BitDepthArg {
    U8,
    U16,
}

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Debug,
    ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum OutputFormat {
    TIFF,
    JPEG, // Lossy, preview only
}

#[derive(
    Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, serde::Serialize, serde::Deserialize,
)]
pub enum BitDepth {
    U8,
    U16,
}


