//! Interruptible reads over ureq's normal TCP, proxy and TLS transports.

use std::io;
use std::time::{Duration, Instant};
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, NextTimeout, Transport,
    time::Duration as TransportDuration,
};

use super::Cancellation;

#[derive(Debug)]
pub(super) struct CancelConnector(pub Cancellation);

impl Connector<Box<dyn Transport>> for CancelConnector {
    type Out = CancelTransport;

    fn connect(
        &self,
        _: &ConnectionDetails,
        transport: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(transport.map(|inner| CancelTransport {
            inner,
            cancel: self.0.clone(),
        }))
    }
}

#[derive(Debug)]
pub(super) struct CancelTransport {
    inner: Box<dyn Transport>,
    cancel: Cancellation,
}

impl Transport for CancelTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let started = Instant::now();
        loop {
            if self.cancel.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "provider request cancelled",
                )
                .into());
            }
            let remaining = if timeout.after.is_not_happening() {
                Duration::MAX
            } else {
                timeout.after.saturating_sub(started.elapsed())
            };
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(timeout.reason));
            }
            let poll = NextTimeout {
                after: TransportDuration::Exact(remaining.min(Duration::from_millis(100))),
                ..timeout
            };
            match self.inner.await_input(poll) {
                // A polling deadline is internal. Preserve the original read
                // deadline and all buffered bytes until progress or cancellation.
                Err(ureq::Error::Timeout(_)) => continue,
                result => return result,
            }
        }
    }

    fn is_open(&mut self) -> bool {
        !self.cancel.is_cancelled() && self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}
