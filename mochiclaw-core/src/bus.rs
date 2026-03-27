//! Message Bus - async channel-based message queue

use crate::error::Error;
use mochiclaw_sdk::message::{InboundMessage, OutboundMessage};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// MessageBus handles all inter-component messaging
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Arc<Mutex<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: Arc<Mutex<mpsc::Receiver<OutboundMessage>>>,
}

impl MessageBus {
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(100);
        let (outbound_tx, outbound_rx) = mpsc::channel(100);
        Self {
            inbound_tx,
            inbound_rx: Arc::new(Mutex::new(inbound_rx)),
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(outbound_rx)),
        }
    }

    pub async fn send_inbound(&self, msg: InboundMessage) -> Result<(), Error> {
        self.inbound_tx
            .send(msg)
            .await
            .map_err(|e| Error::Bus(format!("failed to send inbound: {}", e)))
    }

    pub async fn recv_inbound(&self) -> Option<InboundMessage> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await
    }

    pub async fn send_outbound(&self, msg: OutboundMessage) -> Result<(), Error> {
        self.outbound_tx
            .send(msg)
            .await
            .map_err(|e| Error::Bus(format!("failed to send outbound: {}", e)))
    }

    pub async fn recv_outbound(&self) -> Option<OutboundMessage> {
        let mut rx = self.outbound_rx.lock().await;
        rx.recv().await
    }

    pub fn clone_outbound_sender(&self) -> mpsc::Sender<OutboundMessage> {
        self.outbound_tx.clone()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}
