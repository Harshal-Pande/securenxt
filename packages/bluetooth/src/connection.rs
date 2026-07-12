use async_trait::async_trait;
use securenxt_protocol::{TransportConnection, TransportError};
use btleplug::api::{Peripheral as _, Characteristic, WriteType, ValueNotification};
use btleplug::platform::Peripheral;
use futures::stream::StreamExt;
use std::pin::Pin;

pub struct BluetoothConnection {
    peripheral: Peripheral,
    tx_characteristic: Characteristic,
    #[allow(dead_code)] // Stored to maintain context, though stream provides data
    rx_characteristic: Characteristic,
    notification_stream: Pin<Box<dyn futures::Stream<Item = ValueNotification> + Send>>,
}

impl BluetoothConnection {
    pub fn new(
        peripheral: Peripheral, 
        tx_characteristic: Characteristic, 
        rx_characteristic: Characteristic, 
        notification_stream: Pin<Box<dyn futures::Stream<Item = ValueNotification> + Send>>
    ) -> Self {
        Self {
            peripheral,
            tx_characteristic,
            rx_characteristic,
            notification_stream,
        }
    }
}

#[async_trait]
impl TransportConnection for BluetoothConnection {
    async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.peripheral.write(&self.tx_characteristic, data, WriteType::WithoutResponse)
            .await
            .map_err(|e| TransportError::WriteError(e.to_string()))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        if let Some(notification) = self.notification_stream.next().await {
            Ok(notification.value)
        } else {
            Err(TransportError::ReadError("Notification stream closed".into()))
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.peripheral.disconnect()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))
    }
}
