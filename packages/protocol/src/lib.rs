pub mod protocol {
    include!(concat!(env!("OUT_DIR"), "/securenxt.protocol.rs"));
}

pub mod transport;
pub mod manager;

pub use transport::{Transport, TransportConnection, TransportAdvertisement, TransportError};
pub use manager::TransportManager;
