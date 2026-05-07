#![forbid(unsafe_code)]

#[path = "cli_admin_commands/audit.rs"]
mod audit;
#[path = "cli_admin_commands/backup.rs"]
mod backup;
#[path = "cli_admin_commands/catalog.rs"]
mod catalog;
#[path = "cli_admin_commands/hadr.rs"]
mod hadr;
#[path = "cli_admin_commands/negative_validation.rs"]
mod negative_validation;
#[path = "cli_admin_commands/protocol_vertical.rs"]
mod protocol_vertical;
#[path = "cli_admin_commands/restore.rs"]
mod restore;
#[path = "cli_admin_commands/support.rs"]
mod support;
