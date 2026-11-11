//! AgentHub's screens in the phone app: a board (`/app/boards/{project}`), a ticket
//! (`/app/tickets/{project}/{id}`) and what a person does to one. The board itself and its rules
//! are in `crate::agenthub`; these only show it and act through it.

pub(super) mod board;
pub(super) mod ticket;
pub(super) mod ticket_actions;

#[cfg(test)]
mod ticket_tests;
