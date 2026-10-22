// SPDX-License-Identifier: GPL-3.0-only

//! Shared types, configuration, XDG paths, errors and an injectable clock.
//!
//! It depends on no other workspace crate (SPECIFICATION §5.1, ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cancel;
pub mod clock;
pub mod config;
pub mod error;
pub mod fs;
pub mod paths;

pub use cancel::Cancel;
pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{Config, ConfigError, LookupConfig, PathsConfig, PopupConfig, SubtitlesConfig};
pub use error::Localized;
pub use paths::{Paths, PathsError};
