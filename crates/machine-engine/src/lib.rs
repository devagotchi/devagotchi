//! The Devagotchi machine engine: admission and operation lifecycle (`broker`),
//! native process supervision (`execution`), the durable SQLite journal, the running
//! per-user service (`service`: `devagotchi start`, `devagotchi broker run`).
//! The machine's private AI evidence (`devagotchi ai`) lives in `devagotchi-kernel`.
//! Wire types live in `devagotchi-contracts`; local IPC in `devagotchi-ipc`.
//! The broker modules build where a native execution provider exists; on other
//! targets only `service` is here and the broker binary reports `unsupported_platform`.

pub mod service;

#[cfg(any(target_os = "linux", windows))]
pub mod broker;
#[cfg(any(target_os = "linux", windows))]
pub mod execution;
#[cfg(any(target_os = "linux", windows))]
pub(crate) mod files;
#[cfg(any(target_os = "linux", windows))]
pub mod journal;
#[cfg(any(target_os = "linux", windows))]
pub(crate) mod launch;
#[cfg(any(target_os = "linux", windows))]
pub(crate) mod trace;
