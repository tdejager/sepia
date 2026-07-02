use std::fmt::Display;

use miette::{IntoDiagnostic, Result, WrapErr};

pub trait ResultContextExt<T> {
    fn context<D>(self, msg: D) -> Result<T>
    where
        D: Display + Send + Sync + 'static;

    fn with_context<D, F>(self, msg: F) -> Result<T>
    where
        D: Display + Send + Sync + 'static,
        F: FnOnce() -> D;
}

impl<T, E> ResultContextExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<D>(self, msg: D) -> Result<T>
    where
        D: Display + Send + Sync + 'static,
    {
        self.into_diagnostic().wrap_err(msg)
    }

    fn with_context<D, F>(self, msg: F) -> Result<T>
    where
        D: Display + Send + Sync + 'static,
        F: FnOnce() -> D,
    {
        self.into_diagnostic().wrap_err_with(msg)
    }
}

impl<T> ResultContextExt<T> for Option<T> {
    fn context<D>(self, msg: D) -> Result<T>
    where
        D: Display + Send + Sync + 'static,
    {
        self.ok_or_else(|| miette::miette!("{msg}"))
    }

    fn with_context<D, F>(self, msg: F) -> Result<T>
    where
        D: Display + Send + Sync + 'static,
        F: FnOnce() -> D,
    {
        self.ok_or_else(|| miette::miette!("{}", msg()))
    }
}

pub mod browser;
pub mod config;
pub mod encoder;
pub mod github;
pub mod inspect;
pub mod metadata;
pub mod pr;
pub mod runner;
pub mod session;
pub mod skill_installer;
pub mod timeline;
pub mod uploader;
