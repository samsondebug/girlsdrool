//! Everything that writes outside the database: backups, restore, the full export and the
//! audit pack (ARCHITECTURE §6.6). No network, ever.

pub mod backup;
pub mod full;
pub mod restore;
