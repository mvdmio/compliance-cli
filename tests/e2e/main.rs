//! The E2E suite: Scenarios that run the real `compliance` binary against a Test-bed. Every Scenario is ignored,
//! so `cargo test` lists them and runs none; `cargo test --test e2e -- --ignored` runs them.

mod smoke;
#[path = "../support/mod.rs"]
mod support;
mod test_bed;
mod timing;
