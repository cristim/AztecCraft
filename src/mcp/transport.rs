use super::CloseDelivery;
use eframe::egui;
use futures_util::{SinkExt, StreamExt};
use rmcp::{
    RoleServer,
    model::{
        ClientJsonRpcMessage, ClientNotification, JsonRpcMessage, RequestId, ServerJsonRpcMessage,
        ServerResult,
    },
    transport::{Transport, async_rw::JsonRpcMessageCodec},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, FramedRead, FramedWrite},
};

pub(super) const MAX_INGRESS_BYTES: usize = 64 * 1024;
pub(super) const MAX_EGRESS_BYTES: usize = 1024 * 1024;
pub(super) const MAX_REQUESTS: usize = 16;
pub(super) const MAX_CONTROLS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Control {
    Initialized,
    Cancelled(RequestId),
}

#[derive(Default)]
struct Accounting {
    requests: HashMap<RequestId, bool>,
    controls: HashMap<Control, usize>,
    initialized: bool,
    close_delivery: Option<CloseState>,
}

enum CloseState {
    Waiting(RequestId),
    Flushed,
    Failed,
}

pub(super) struct Shared {
    #[cfg(test)]
    read_observed: AtomicBool,
    #[cfg(test)]
    forwarded_notifications: std::sync::atomic::AtomicUsize,
    alive: AtomicBool,
    accounting: Mutex<Accounting>,
    context: egui::Context,
}

impl Shared {
    pub(super) fn new(context: egui::Context) -> Self {
        Self {
            #[cfg(test)]
            read_observed: AtomicBool::new(false),
            #[cfg(test)]
            forwarded_notifications: std::sync::atomic::AtomicUsize::new(0),
            alive: AtomicBool::new(true),
            accounting: Mutex::new(Accounting::default()),
            context,
        }
    }

    pub(super) fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn read_observed(&self) -> bool {
        self.read_observed.load(Ordering::Acquire)
    }

    pub(super) fn repaint(&self) {
        self.context.request_repaint();
    }

    pub(super) fn close(&self) {
        self.alive.store(false, Ordering::Release);
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        accounting.requests.clear();
        accounting.controls.clear();
        if matches!(accounting.close_delivery, Some(CloseState::Waiting(_))) {
            accounting.close_delivery = Some(CloseState::Failed);
        }
        drop(accounting);
        self.repaint();
    }

    fn admit(&self, message: &ClientJsonRpcMessage) -> Result<bool, ()> {
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        if !self.alive() {
            return Err(());
        }
        match message {
            JsonRpcMessage::Request(request) => {
                if accounting.requests.len() >= MAX_REQUESTS
                    || accounting.requests.contains_key(&request.id)
                {
                    return Err(());
                }
                accounting.requests.insert(request.id.clone(), false);
                Ok(true)
            }
            JsonRpcMessage::Notification(notification) => {
                let mut close_failed = false;
                let control = match &notification.notification {
                    ClientNotification::InitializedNotification(_) => {
                        if accounting.initialized {
                            return Ok(false);
                        }
                        accounting.initialized = true;
                        Control::Initialized
                    }
                    ClientNotification::CancelledNotification(notification) => {
                        let Some(id) = &notification.params.request_id else {
                            return Ok(false);
                        };
                        let Some(cancelled) = accounting.requests.get_mut(id) else {
                            return Ok(false);
                        };
                        if *cancelled {
                            return Ok(false);
                        }
                        *cancelled = true;
                        if matches!(&accounting.close_delivery, Some(CloseState::Waiting(waiting)) if waiting == id)
                        {
                            accounting.close_delivery = Some(CloseState::Failed);
                            close_failed = true;
                        }
                        Control::Cancelled(id.clone())
                    }
                    _ => return Ok(false),
                };
                if accounting.controls.values().sum::<usize>() >= MAX_CONTROLS {
                    return Err(());
                }
                *accounting.controls.entry(control).or_default() += 1;
                drop(accounting);
                if close_failed {
                    self.repaint();
                }
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn wrote(&self, id: Option<RequestId>, successful_tool: bool) {
        if let Some(id) = id {
            let mut accounting = self
                .accounting
                .lock()
                .expect("MCP accounting lock poisoned");
            let cancelled = accounting.requests.remove(&id);
            let matches_close = matches!(&accounting.close_delivery, Some(CloseState::Waiting(waiting)) if waiting == &id);
            if matches_close {
                accounting.close_delivery = Some(
                    if successful_tool && cancelled == Some(false) && self.alive() {
                        CloseState::Flushed
                    } else {
                        CloseState::Failed
                    },
                );
            }
            drop(accounting);
            if matches_close {
                self.repaint();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn test_force_close_delivery(&self, delivery: super::CloseDelivery) {
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        accounting.close_delivery = Some(match delivery {
            super::CloseDelivery::Waiting => CloseState::Waiting(RequestId::Number(-1)),
            super::CloseDelivery::Flushed => CloseState::Flushed,
            super::CloseDelivery::Failed => CloseState::Failed,
        });
    }

    pub(super) fn arm_close<T>(&self, id: RequestId, reply: &tokio::sync::oneshot::Sender<T>) {
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        let failed =
            !self.alive() || accounting.requests.get(&id) != Some(&false) || reply.is_closed();
        accounting.close_delivery = Some(if failed {
            CloseState::Failed
        } else {
            CloseState::Waiting(id)
        });
        drop(accounting);
        if failed {
            self.repaint();
        }
    }

    pub(super) fn fail_close(&self, id: &RequestId) {
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        let waiting = matches!(&accounting.close_delivery, Some(CloseState::Waiting(waiting)) if waiting == id);
        if waiting {
            accounting.close_delivery = Some(CloseState::Failed);
        }
        drop(accounting);
        if waiting {
            self.repaint();
        }
    }

    pub(super) fn poll_close_delivery(&self) -> Option<CloseDelivery> {
        let mut accounting = self
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        match accounting.close_delivery.as_ref()? {
            CloseState::Waiting(_) => Some(CloseDelivery::Waiting),
            CloseState::Flushed => {
                accounting.close_delivery = None;
                Some(CloseDelivery::Flushed)
            }
            CloseState::Failed => {
                accounting.close_delivery = None;
                Some(CloseDelivery::Failed)
            }
        }
    }

    pub(super) fn complete_control(self: &Arc<Self>, control: Control) -> ControlCompletion {
        ControlCompletion {
            shared: self.clone(),
            control,
        }
    }
}

pub(super) struct ControlCompletion {
    shared: Arc<Shared>,
    control: Control,
}

impl Drop for ControlCompletion {
    fn drop(&mut self) {
        let mut accounting = self
            .shared
            .accounting
            .lock()
            .expect("MCP accounting lock poisoned");
        if let Some(count) = accounting.controls.get_mut(&self.control) {
            *count -= 1;
            if *count == 0 {
                accounting.controls.remove(&self.control);
            }
        }
    }
}

struct ProgressCodec(JsonRpcMessageCodec<ClientJsonRpcMessage>);

impl Decoder for ProgressCodec {
    type Item = ClientJsonRpcMessage;
    type Error = <JsonRpcMessageCodec<ClientJsonRpcMessage> as Decoder>::Error;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            let before = source.len();
            let message = self.0.decode(source)?;
            if message.is_some() || source.len() == before {
                return Ok(message);
            }
        }
    }

    fn decode_eof(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.decode(source)? {
            Some(message) => Ok(Some(message)),
            None => self.0.decode_eof(source),
        }
    }
}

pub(super) struct BoundedTransport<R, W> {
    reader: FramedRead<R, ProgressCodec>,
    writer: Arc<tokio::sync::Mutex<FramedWrite<W, JsonRpcMessageCodec<ServerJsonRpcMessage>>>>,
    shared: Arc<Shared>,
}

impl<R, W> BoundedTransport<R, W> {
    pub(super) fn new(reader: R, writer: W, shared: Arc<Shared>) -> Self {
        Self {
            reader: FramedRead::new(
                reader,
                ProgressCodec(JsonRpcMessageCodec::new_with_max_length(MAX_INGRESS_BYTES)),
            ),
            writer: Arc::new(tokio::sync::Mutex::new(FramedWrite::new(
                writer,
                JsonRpcMessageCodec::new(),
            ))),
            shared,
        }
    }
}

impl<R, W> Transport<RoleServer> for BoundedTransport<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    type Error = io::Error;

    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let writer = self.writer.clone();
        let shared = self.shared.clone();
        async move {
            let id = match &message {
                JsonRpcMessage::Response(response) => Some(response.id.clone()),
                JsonRpcMessage::Error(error) => error.id.clone(),
                _ => None,
            };
            let successful_tool = matches!(&message, JsonRpcMessage::Response(response)
                if matches!(&response.result, ServerResult::CallToolResult(result) if result.is_error != Some(true)));
            if check_serialized_size(&message, MAX_EGRESS_BYTES - 1).is_err() {
                eprintln!("MCP reply exceeds transport limit; closing connection");
                shared.close();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP reply too large",
                ));
            }
            let result = writer
                .lock()
                .await
                .send(message)
                .await
                .map_err(io::Error::from);
            if result.is_ok() {
                shared.wrote(id, successful_tool);
            } else {
                shared.close();
            }
            result
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if !self.shared.alive() {
                return None;
            }
            #[cfg(test)]
            self.shared.read_observed.store(true, Ordering::Release);
            match self.reader.next().await {
                Some(Ok(message)) => match self.shared.admit(&message) {
                    Ok(true) => {
                        #[cfg(test)]
                        if matches!(message, JsonRpcMessage::Notification(_)) {
                            self.shared
                                .forwarded_notifications
                                .fetch_add(1, Ordering::SeqCst);
                        }
                        return Some(message);
                    }
                    Ok(false) => tokio::task::yield_now().await,
                    Err(()) => {
                        eprintln!("MCP admission limit or duplicate request; closing connection");
                        self.shared.close();
                        return None;
                    }
                },
                Some(Err(_)) => {
                    eprintln!("Invalid MCP ingress; closing connection");
                    self.shared.close();
                    return None;
                }
                None => {
                    self.shared.close();
                    return None;
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.shared.close();
        Ok(())
    }
}

struct ByteCounter {
    remaining: usize,
}

impl io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.remaining = self.remaining.checked_sub(bytes.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "serialized result too large")
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn check_serialized_size<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(ByteCounter { remaining: limit }, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(value: serde_json::Value) -> ClientJsonRpcMessage {
        serde_json::from_value(value).unwrap()
    }

    fn request(id: i64) -> ClientJsonRpcMessage {
        message(json!({"jsonrpc":"2.0", "id":id, "method":"ping"}))
    }

    fn cancelled(id: i64) -> ClientJsonRpcMessage {
        message(
            json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":id}}),
        )
    }

    #[test]
    fn close_delivery_rejects_before_arm_cancellation_absence_and_dead_transport() {
        for condition in 0..3 {
            let shared = Shared::new(egui::Context::default());
            if condition != 1 {
                shared.admit(&request(1)).unwrap();
            }
            if condition == 0 {
                shared.admit(&cancelled(1)).unwrap();
            } else if condition == 2 {
                shared.close();
            }
            let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
            shared.arm_close(RequestId::Number(1), &reply);
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Failed));
            assert_eq!(shared.poll_close_delivery(), None);
        }
        let shared = Shared::new(egui::Context::default());
        shared.admit(&request(1)).unwrap();
        let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
        shared.arm_close(RequestId::Number(1), &reply);
        shared.admit(&cancelled(1)).unwrap();
        assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Failed));
    }

    fn tool_response(id: i64, error: bool) -> ServerJsonRpcMessage {
        let mut result =
            rmcp::model::CallToolResult::structured(json!({"outcome":"close_requested"}));
        result.is_error = Some(error);
        JsonRpcMessage::Response(rmcp::model::JsonRpcResponse {
            jsonrpc: Default::default(),
            id: RequestId::Number(id),
            result: ServerResult::CallToolResult(result),
        })
    }

    #[test]
    fn old_closed_receiver_cannot_arm_a_reused_request_id() {
        let shared = Shared::new(egui::Context::default());
        shared.admit(&request(1)).unwrap();
        let (reply, receiver) = tokio::sync::oneshot::channel::<()>();
        drop(receiver);
        shared.wrote(Some(RequestId::Number(1)), false);
        shared.admit(&request(1)).unwrap();
        shared.arm_close(RequestId::Number(1), &reply);
        shared.wrote(Some(RequestId::Number(1)), true);
        assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Failed));
    }

    struct ControlledFlush {
        bytes: Arc<Mutex<Vec<u8>>>,
        entered: Arc<AtomicBool>,
        completion: tokio::sync::oneshot::Receiver<Result<(), io::ErrorKind>>,
    }

    impl AsyncWrite for ControlledFlush {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            bytes: &[u8],
        ) -> std::task::Poll<io::Result<usize>> {
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            std::task::Poll::Ready(Ok(bytes.len()))
        }

        fn poll_flush(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            self.entered.store(true, Ordering::Release);
            match std::pin::Pin::new(&mut self.completion).poll(cx) {
                std::task::Poll::Pending => std::task::Poll::Pending,
                std::task::Poll::Ready(Ok(result)) => {
                    std::task::Poll::Ready(result.map_err(io::Error::from))
                }
                std::task::Poll::Ready(Err(_)) => {
                    std::task::Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
                }
            }
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn accepting_every_byte_does_not_approve_close_before_successful_flush() {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            for succeeds in [false, true] {
                let shared = Arc::new(Shared::new(egui::Context::default()));
                shared.admit(&request(1)).unwrap();
                let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
                shared.arm_close(RequestId::Number(1), &reply);
                let bytes = Arc::new(Mutex::new(Vec::new()));
                let entered = Arc::new(AtomicBool::new(false));
                let (completion, receiver) = tokio::sync::oneshot::channel();
                let writer = ControlledFlush {
                    bytes: bytes.clone(),
                    entered: entered.clone(),
                    completion: receiver,
                };
                let mut transport =
                    BoundedTransport::new(tokio::io::empty(), writer, shared.clone());
                let sending = tokio::spawn(transport.send(tool_response(1, false)));
                while !entered.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
                let captured = bytes.lock().unwrap().clone();
                let response: serde_json::Value = serde_json::from_slice(&captured).unwrap();
                assert_eq!(response["id"], 1);
                assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Waiting));
                completion
                    .send(if succeeds {
                        Ok(())
                    } else {
                        Err(io::ErrorKind::BrokenPipe)
                    })
                    .unwrap();
                assert_eq!(sending.await.unwrap().is_ok(), succeeds);
                assert_eq!(
                    shared.poll_close_delivery(),
                    Some(if succeeds {
                        CloseDelivery::Flushed
                    } else {
                        CloseDelivery::Failed
                    })
                );
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn close_delivery_requires_the_matching_successful_flushed_tool_response() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            shared.admit(&request(1)).unwrap();
            shared.admit(&request(2)).unwrap();
            let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
            shared.arm_close(RequestId::Number(1), &reply);
            let mut unrelated =
                BoundedTransport::new(tokio::io::empty(), tokio::io::sink(), shared.clone());
            unrelated.send(tool_response(2, false)).await.unwrap();
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Waiting));
            let (writer, mut reader) = tokio::io::duplex(1);
            let mut transport = BoundedTransport::new(tokio::io::empty(), writer, shared.clone());
            let sending = tokio::spawn(transport.send(tool_response(1, false)));
            let mut first = [0; 1];
            reader.read_exact(&mut first).await.unwrap();
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Waiting));
            let mut reader = tokio::io::BufReader::new(reader);
            let mut complete = first.to_vec();
            reader.read_until(b'\n', &mut complete).await.unwrap();
            sending.await.unwrap().unwrap();
            let reply: serde_json::Value = serde_json::from_slice(&complete).unwrap();
            assert_eq!(reply["id"], 1);
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Flushed));
            assert_eq!(shared.poll_close_delivery(), None);
        })
        .await
        .unwrap();
        for error in [true, false] {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            shared.admit(&request(1)).unwrap();
            let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
            shared.arm_close(RequestId::Number(1), &reply);
            let mut transport =
                BoundedTransport::new(tokio::io::empty(), tokio::io::sink(), shared.clone());
            if error {
                transport.send(tool_response(1, true)).await.unwrap();
            } else {
                transport
                    .send(JsonRpcMessage::Response(rmcp::model::JsonRpcResponse {
                        jsonrpc: Default::default(),
                        id: RequestId::Number(1),
                        result: ServerResult::empty(()),
                    }))
                    .await
                    .unwrap();
            }
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Failed));
        }
    }

    #[tokio::test]
    async fn close_delivery_write_failure_and_cancelled_flush_never_approve_closure() {
        use tokio::io::AsyncReadExt;
        for cancelled_flush in [false, true] {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            shared.admit(&request(1)).unwrap();
            let (reply, _receiver) = tokio::sync::oneshot::channel::<()>();
            shared.arm_close(RequestId::Number(1), &reply);
            let (writer, mut reader) = tokio::io::duplex(1);
            let mut transport = BoundedTransport::new(tokio::io::empty(), writer, shared.clone());
            let sending = tokio::spawn(transport.send(tool_response(1, false)));
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                reader.read_exact(&mut [0; 1]),
            )
            .await
            .unwrap()
            .unwrap();
            if cancelled_flush {
                shared.admit(&cancelled(1)).unwrap();
            }
            drop(reader);
            assert!(sending.await.unwrap().is_err());
            assert_eq!(shared.poll_close_delivery(), Some(CloseDelivery::Failed));
        }
    }

    #[test]
    fn rpc_slots_remain_until_written_even_after_cancel_and_handler_completion() {
        let shared = Shared::new(egui::Context::default());
        for id in 0..MAX_REQUESTS as i64 {
            assert_eq!(shared.admit(&request(id)), Ok(true));
        }
        assert_eq!(shared.admit(&request(100)), Err(()));
        assert_eq!(shared.admit(&cancelled(0)), Ok(true));
        assert_eq!(shared.admit(&cancelled(0)), Ok(false));
        assert_eq!(shared.admit(&request(100)), Err(()));
        shared.wrote(Some(RequestId::Number(1)), false);
        assert_eq!(shared.admit(&request(100)), Ok(true));
        assert_eq!(shared.admit(&request(100)), Err(()));
        shared.close();
        assert!(shared.accounting.lock().unwrap().requests.is_empty());
        assert_eq!(shared.admit(&request(200)), Err(()));
    }

    #[test]
    fn notification_permits_survive_response_winning_race_and_release_on_callback_drop() {
        let shared = Arc::new(Shared::new(egui::Context::default()));
        assert_eq!(shared.admit(&request(0)), Ok(true));
        assert_eq!(shared.admit(&cancelled(0)), Ok(true));
        let completion = shared.complete_control(Control::Cancelled(RequestId::Number(0)));
        shared.wrote(Some(RequestId::Number(0)), false);
        assert_eq!(shared.accounting.lock().unwrap().controls.len(), 1);
        drop(completion);
        assert!(shared.accounting.lock().unwrap().controls.is_empty());
        for id in 1..=MAX_CONTROLS as i64 {
            assert_eq!(shared.admit(&request(id)), Ok(true));
            assert_eq!(shared.admit(&cancelled(id)), Ok(true));
            shared.wrote(Some(RequestId::Number(id)), false);
        }
        assert_eq!(shared.admit(&request(100)), Ok(true));
        assert_eq!(shared.admit(&cancelled(100)), Err(()));
        shared.close();
        assert!(shared.accounting.lock().unwrap().controls.is_empty());
    }

    #[test]
    fn reused_request_ids_keep_each_pending_cancel_callback_permit() {
        let shared = Arc::new(Shared::new(egui::Context::default()));
        let pending = || {
            shared
                .accounting
                .lock()
                .unwrap()
                .controls
                .values()
                .sum::<usize>()
        };
        for first_completed in [0, 1] {
            let mut pair = Vec::new();
            for _ in 0..2 {
                assert_eq!(shared.admit(&request(0)), Ok(true));
                assert_eq!(shared.admit(&cancelled(0)), Ok(true));
                pair.push(shared.complete_control(Control::Cancelled(RequestId::Number(0))));
                shared.wrote(Some(RequestId::Number(0)), false);
            }
            assert_eq!(pending(), 2);
            drop(pair.remove(first_completed));
            assert_eq!(pending(), 1);
            drop(pair);
            assert_eq!(pending(), 0);
        }
        let mut callbacks = Vec::new();
        for count in 1..=MAX_CONTROLS {
            assert_eq!(shared.admit(&request(0)), Ok(true));
            assert_eq!(shared.admit(&cancelled(0)), Ok(true));
            callbacks.push(shared.complete_control(Control::Cancelled(RequestId::Number(0))));
            shared.wrote(Some(RequestId::Number(0)), false);
            let permits = shared
                .accounting
                .lock()
                .unwrap()
                .controls
                .values()
                .sum::<usize>();
            assert_eq!(permits, count);
        }
        drop(callbacks.remove(0));
        let permits = shared
            .accounting
            .lock()
            .unwrap()
            .controls
            .values()
            .sum::<usize>();
        assert_eq!(permits, MAX_CONTROLS - 1);
        assert_eq!(shared.admit(&request(0)), Ok(true));
        assert_eq!(shared.admit(&cancelled(0)), Ok(true));
        callbacks.push(shared.complete_control(Control::Cancelled(RequestId::Number(0))));
        shared.wrote(Some(RequestId::Number(0)), false);
        assert_eq!(shared.admit(&request(0)), Ok(true));
        assert_eq!(shared.admit(&cancelled(0)), Err(()));
        shared.close();
        drop(callbacks);
        assert!(shared.accounting.lock().unwrap().controls.is_empty());
    }

    #[test]
    fn unsupported_and_duplicate_control_floods_never_reach_sdk_dispatch() {
        let shared = Arc::new(Shared::new(egui::Context::default()));
        let initialized = message(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
        let progress = message(
            json!({"jsonrpc":"2.0", "method":"notifications/progress", "params":{"progressToken":1,"progress":0}}),
        );
        assert_eq!(shared.admit(&initialized), Ok(true));
        drop(shared.complete_control(Control::Initialized));
        for _ in 0..10_000 {
            assert_eq!(shared.admit(&initialized), Ok(false));
            assert_eq!(shared.admit(&cancelled(123)), Ok(false));
            assert_eq!(shared.admit(&progress), Ok(false));
        }
        assert!(shared.accounting.lock().unwrap().requests.is_empty());
        assert!(shared.accounting.lock().unwrap().controls.is_empty());
    }

    #[tokio::test]
    async fn actual_sdk_filters_notification_flood_before_handler_tasks() {
        use rmcp::{ServerHandler, ServiceExt, service::NotificationContext};
        use std::sync::atomic::AtomicUsize;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        struct CountingServer {
            shared: Arc<Shared>,
            initialized: Arc<AtomicUsize>,
            progress: Arc<AtomicUsize>,
        }
        impl ServerHandler for CountingServer {
            fn on_initialized(
                &self,
                _context: NotificationContext<RoleServer>,
            ) -> impl Future<Output = ()> + Send + '_ {
                let completion = self.shared.complete_control(Control::Initialized);
                self.initialized.fetch_add(1, Ordering::SeqCst);
                async move { drop(completion) }
            }

            fn on_progress(
                &self,
                _notification: rmcp::model::ProgressNotificationParam,
                _context: NotificationContext<RoleServer>,
            ) -> impl Future<Output = ()> + Send + '_ {
                self.progress.fetch_add(1, Ordering::SeqCst);
                std::future::ready(())
            }
        }

        // Liveness guard only: the assertions below are count and ordering based.
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            let initialized = Arc::new(AtomicUsize::new(0));
            let progress = Arc::new(AtomicUsize::new(0));
            let (client, server) = tokio::io::duplex(MAX_INGRESS_BYTES);
            let (server_reader, server_writer) = tokio::io::split(server);
            let (client_reader, mut client_writer) = tokio::io::split(client);
            let mut replies = BufReader::new(client_reader).lines();
            let server = CountingServer {
                shared: shared.clone(),
                initialized: initialized.clone(),
                progress: progress.clone(),
            };
            let transport_shared = shared.clone();
            let serving = tokio::spawn(async move {
                server
                    .serve(BoundedTransport::new(server_reader, server_writer, transport_shared))
                    .await
                    .unwrap()
                    .waiting()
                    .await
                    .unwrap();
            });
            client_writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"flood-test\",\"version\":\"1\"}}}\n").await.unwrap();
            assert!(replies.next_line().await.unwrap().unwrap().contains("\"result\""));
            client_writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await.unwrap();
            while initialized.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            for _ in 0..10_000 {
                client_writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"progressToken\":1,\"progress\":0}}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":123}}\n").await.unwrap();
            }
            client_writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n").await.unwrap();
            let reply: serde_json::Value = serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(reply["id"], 1);
            assert_eq!(initialized.load(Ordering::SeqCst), 1);
            // The ping is read after every flood frame on the same receive loop, so this
            // counts what reached the SDK without racing its spawned notification handlers.
            assert_eq!(shared.forwarded_notifications.load(Ordering::SeqCst), 1);
            client_writer.shutdown().await.unwrap();
            serving.await.unwrap();
        }).await.unwrap();
    }

    #[tokio::test]
    async fn stalled_stdout_keeps_all_rpc_slots_and_failure_closes_bridge() {
        let shared = Arc::new(Shared::new(egui::Context::default()));
        for id in 0..MAX_REQUESTS as i64 {
            shared.admit(&request(id)).unwrap();
        }
        let (writer, reader) = tokio::io::duplex(1);
        let mut transport = BoundedTransport::new(tokio::io::empty(), writer, shared.clone());
        let response: ServerJsonRpcMessage =
            serde_json::from_value(json!({"jsonrpc":"2.0","id":0,"result":{}})).unwrap();
        let sending = tokio::spawn(transport.send(response));
        tokio::task::yield_now().await;
        assert_eq!(shared.admit(&request(100)), Err(()));
        assert_eq!(
            shared.accounting.lock().unwrap().requests.len(),
            MAX_REQUESTS
        );
        drop(reader);
        assert!(sending.await.unwrap().is_err());
        assert!(!shared.alive());
        assert!(shared.accounting.lock().unwrap().requests.is_empty());
    }

    #[tokio::test]
    async fn eof_and_oversized_frames_close_before_sdk_can_drain() {
        use tokio::io::AsyncWriteExt;
        let shared = Arc::new(Shared::new(egui::Context::default()));
        let mut transport =
            BoundedTransport::new(tokio::io::empty(), tokio::io::sink(), shared.clone());
        assert!(transport.receive().await.is_none());
        assert!(!shared.alive());
        let shared = Arc::new(Shared::new(egui::Context::default()));
        let (mut source, reader) = tokio::io::duplex(MAX_INGRESS_BYTES * 2);
        source
            .write_all(&vec![b'x'; MAX_INGRESS_BYTES + 1])
            .await
            .unwrap();
        let mut transport = BoundedTransport::new(reader, tokio::io::sink(), shared.clone());
        assert!(transport.receive().await.is_none());
        assert!(!shared.alive());
    }

    #[tokio::test]
    async fn ingress_accepts_exact_limit_and_rejects_oversize_or_nonfinite_json() {
        use tokio::io::AsyncWriteExt;
        let prefix = b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"ping\",\"padding\":\"";
        for length in [MAX_INGRESS_BYTES, MAX_INGRESS_BYTES + 1] {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            let (mut source, reader) = tokio::io::duplex(MAX_INGRESS_BYTES * 2);
            let mut frame = prefix.to_vec();
            frame.resize(length - 2, b'x');
            frame.extend_from_slice(b"\"}\n");
            assert_eq!(frame.len(), length + 1);
            source.write_all(&frame).await.unwrap();
            let mut transport = BoundedTransport::new(reader, tokio::io::sink(), shared.clone());
            assert_eq!(
                transport.receive().await.is_some(),
                length == MAX_INGRESS_BYTES
            );
            assert_eq!(shared.alive(), length == MAX_INGRESS_BYTES);
        }
        let shared = Arc::new(Shared::new(egui::Context::default()));
        let (mut source, reader) = tokio::io::duplex(256);
        source
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"ping\",\"params\":{\"x\":NaN}}\n",
            )
            .await
            .unwrap();
        let mut transport = BoundedTransport::new(reader, tokio::io::sink(), shared.clone());
        assert!(transport.receive().await.is_none());
        assert!(!shared.alive());
    }

    #[tokio::test]
    async fn sdk_ignored_frames_do_not_stall_coalesced_ping_or_grow_buffer() {
        use tokio::io::AsyncWriteExt;
        for ignored_count in [2_000, 1] {
            let shared = Arc::new(Shared::new(egui::Context::default()));
            let (mut source, reader) = tokio::io::duplex(MAX_INGRESS_BYTES * 4);
            let ignored = b"{\"jsonrpc\":\"2.0\",\"method\":\"nonstandard\",\"params\":1}\n";
            let mut frames = ignored.repeat(ignored_count);
            frames.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
            source.write_all(&frames).await.unwrap();
            let mut transport = BoundedTransport::new(reader, tokio::io::sink(), shared.clone());
            let received =
                tokio::time::timeout(std::time::Duration::from_secs(2), transport.receive()).await;
            assert!(
                received.is_ok(),
                "coalesced ping stalled after {ignored_count} SDK-ignored frames; buffered {} bytes",
                transport.reader.read_buffer().len()
            );
            assert!(
                matches!(received.unwrap(), Some(JsonRpcMessage::Request(request)) if request.id == RequestId::Number(7))
            );
            assert!(transport.reader.read_buffer().len() <= MAX_INGRESS_BYTES);
            assert!(shared.alive());
        }
    }

    #[test]
    fn eof_progress_skips_complete_ignored_frames_before_partial_final_ping() {
        let mut codec = ProgressCodec(JsonRpcMessageCodec::new_with_max_length(MAX_INGRESS_BYTES));
        let mut source = BytesMut::from(&b"{\"jsonrpc\":\"2.0\",\"method\":\"nonstandard\",\"params\":1}\n{\"jsonrpc\":\"2.0\",\"method\":\"nonstandard\",\"params\":1}\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}"[..]);
        assert!(
            matches!(codec.decode_eof(&mut source).unwrap(), Some(JsonRpcMessage::Request(request)) if request.id == RequestId::Number(7))
        );
        assert!(codec.decode_eof(&mut source).unwrap().is_none());
        assert!(source.is_empty());
    }

    #[test]
    fn serialized_counter_enforces_utf8_and_escaped_content_boundaries() {
        assert!(check_serialized_size(&"é", 4).is_ok());
        assert!(check_serialized_size(&"é", 3).is_err());
        assert!(check_serialized_size(&"\n", 4).is_ok());
        assert!(check_serialized_size(&"\n", 3).is_err());
    }
}
