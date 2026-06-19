// Library target — exposes internal modules for integration tests.
// The binary entry point remains main.rs.
pub mod audit;
pub mod config;
pub mod crawler;
pub mod error;
pub mod fetcher;
pub mod model;
pub mod parser;
pub mod pipeline;
pub mod report;
pub mod score;
pub mod server;
