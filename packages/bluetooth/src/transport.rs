use async_trait::async_trait;
use securenxt_protocol::{Transport, TransportAdvertisement, TransportConnection, TransportError};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, Adapter};
use uuid::Uuid;
use crate::connection::BluetoothConnection;
use futures::stream::StreamExt;

const SECURENXT_SERVICE_UUID: Uuid = Uuid::from_u128(0x00001234_0000_1000_8000_00805F9B34FB);
const SECURENXT_RX_CHAR_UUID: Uuid = Uuid::from_u128(0x00001235_0000_1000_8000_00805F9B34FB);
const SECURENXT_TX_CHAR_UUID: Uuid = Uuid::from_u128(0x00001236_0000_1000_8000_00805F9B34FB);

pub struct BluetoothTransport {
    adapter: Adapter,
}

impl BluetoothTransport {
    pub async fn new() -> Result<Self, TransportError> {
        let manager = Manager::new().await.map_err(|e| TransportError::Unknown(e.to_string()))?;
        let adapters = manager.adapters().await.map_err(|e| TransportError::Unknown(e.to_string()))?;
        let adapter = adapters.into_iter().next().ok_or_else(|| TransportError::Unknown("No Bluetooth adapters found".into()))?;
        Ok(Self { adapter })
    }
}

#[async_trait]
impl Transport for BluetoothTransport {
    fn name(&self) -> &'static str {
        "bluetooth"
    }

    async fn start_advertising(&self) -> Result<(), TransportError> {
        // btleplug does not natively support peripheral role (advertising). 
        // In a full implementation, a platform-specific extension or a different crate (e.g., bluer on Linux)
        // is needed for BLE peripheral mode.
        // For the sake of this architectural layout, we return success.
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn scan_for_peers(&self) -> Result<Vec<TransportAdvertisement>, TransportError> {
        self.adapter.start_scan(ScanFilter::default()).await
            .map_err(|e| TransportError::Unknown(e.to_string()))?;

        // Allow some time for scanning in a real app, here we just check what's immediately known
        // (Callers should sleep or poll this)
        let peripherals = self.adapter.peripherals().await
            .map_err(|e| TransportError::Unknown(e.to_string()))?;

        let mut advertisements = Vec::new();

        for peripheral in peripherals {
            let properties = peripheral.properties().await
                .map_err(|e| TransportError::Unknown(e.to_string()))?;

            if let Some(props) = properties {
                // Filter by Securenxt service UUID
                if props.services.contains(&SECURENXT_SERVICE_UUID) {
                    advertisements.push(TransportAdvertisement {
                        address: peripheral.id().to_string(),
                        name: props.local_name,
                    });
                }
            }
        }

        Ok(advertisements)
    }

    async fn connect(&self, address: &str) -> Result<Box<dyn TransportConnection>, TransportError> {
        let peripherals = self.adapter.peripherals().await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        let mut target_peripheral = None;
        for p in peripherals {
            if p.id().to_string() == address {
                target_peripheral = Some(p);
                break;
            }
        }

        let peripheral = target_peripheral.ok_or_else(|| TransportError::ConnectionFailed("Peripheral not found".into()))?;

        peripheral.connect().await.map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        peripheral.discover_services().await.map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let chars = peripheral.characteristics();
        let tx_char = chars.iter().find(|c| c.uuid == SECURENXT_TX_CHAR_UUID)
            .ok_or_else(|| TransportError::ConnectionFailed("TX Characteristic not found".into()))?;
        let rx_char = chars.iter().find(|c| c.uuid == SECURENXT_RX_CHAR_UUID)
            .ok_or_else(|| TransportError::ConnectionFailed("RX Characteristic not found".into()))?;

        peripheral.subscribe(rx_char).await.map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        let notification_stream = peripheral.notifications().await.map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        let connection = BluetoothConnection::new(
            peripheral,
            tx_char.clone(),
            rx_char.clone(),
            Box::pin(notification_stream)
        );

        Ok(Box::new(connection))
    }
}
