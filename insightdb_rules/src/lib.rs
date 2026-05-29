pub mod models;

mod full_table_scan;
mod missing_index;
mod filesort;
mod temporary_table;
mod nested_loop_risk;
mod abnormal_scan_rows;

pub mod engine;

pub use models::*;
pub use engine::{run_rules, all_rules};
