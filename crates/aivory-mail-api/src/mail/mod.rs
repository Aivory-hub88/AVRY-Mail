pub mod inbound;
pub mod outbound;
pub mod cloudflare;
pub mod cognee_client;
pub mod dns_check;
pub mod dkim;
pub mod routing;

pub use inbound::handle_inbound_raw;
pub use outbound::send_email;
