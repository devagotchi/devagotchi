//! AgentHub's screens in the phone app: a board (`/app/boards/{project}`), a ticket
//! (`/app/tickets/{project}/{id}`) and what a person does to one. The board itself and its rules
//! are in `crate::agenthub`; these only show it and act through it.

pub(super) mod audit;
pub(super) mod board;
pub(super) mod configuration;
pub(super) mod create;
pub(super) mod projects;
pub(super) mod ticket;
pub(super) mod ticket_actions;

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod ticket_tests;
