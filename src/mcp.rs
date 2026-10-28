mod args;
mod features;
mod registry;
mod tools;
mod transport;

#[cfg(test)]
pub(crate) use registry::tool_names;
#[cfg(all(test, feature = "mcp-test-client"))]
pub(crate) use transport::MAX_EGRESS_BYTES;

use crate::{
    app::Editor,
    editor_operations::{OperationError, OperationReply, RemoteRequest},
};
use eframe::egui;
use rmcp::ServiceExt;
use rmcp::model::RequestId;
use std::{
    io,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use transport::{BoundedTransport, Shared};

const UI_QUEUE_CAPACITY: usize = 16;
const UI_REQUESTS_PER_FRAME: usize = 8;
const UI_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

struct EditorEnvelope {
    request: RemoteRequest,
    reply: oneshot::Sender<Result<OperationReply, OperationError>>,
    cancellation: CancellationToken,
    request_id: RequestId,
    deadline: Instant,
    #[cfg(test)]
    install_probe: Option<Box<dyn Fn() + Send>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseDelivery {
    Waiting,
    Flushed,
    Failed,
}

pub(crate) struct UiBridge {
    receiver: mpsc::Receiver<EditorEnvelope>,
    shared: Arc<Shared>,
    stop: CancellationToken,
    thread: Option<thread::JoinHandle<()>>,
    close_requested: bool,
}

impl UiBridge {
    pub(crate) fn poll_close_delivery(&mut self) -> Option<CloseDelivery> {
        let delivery = self.shared.poll_close_delivery();
        if delivery == Some(CloseDelivery::Failed) {
            self.close_requested = true;
        }
        delivery
    }
    #[cfg(test)]
    pub(crate) fn test_read_observed(&self) -> bool {
        self.shared.read_observed()
    }

    #[cfg(test)]
    pub(crate) fn test_queue_len(&self) -> usize {
        self.receiver.len()
    }

    #[cfg(test)]
    pub(crate) fn test_alive(&self) -> bool {
        self.shared.alive()
    }
    pub(crate) fn start(context: egui::Context) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(UI_QUEUE_CAPACITY);
        let shared = Arc::new(Shared::new(context));
        let stop = CancellationToken::new();
        let thread_shared = shared.clone();
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("azteccraft-mcp".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("MCP runtime startup failed: {error}");
                        thread_shared.close();
                        return;
                    }
                };
                runtime.block_on(async {
                    let server = tools::EditorServer::new(sender, thread_shared.clone());
                    let transport = BoundedTransport::new(
                        tokio::io::stdin(),
                        tokio::io::stdout(),
                        thread_shared.clone(),
                    );
                    let serving = async {
                        match server.serve(transport).await {
                            Ok(service) => {
                                if service.waiting().await.is_err() {
                                    eprintln!("MCP service task failed");
                                }
                            }
                            Err(_) => eprintln!("MCP initialization failed or connection closed"),
                        }
                    };
                    tokio::select! {
                        _ = thread_stop.cancelled() => (),
                        _ = serving => (),
                    }
                });
                thread_shared.close();
                runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
            })?;
        Ok(Self {
            receiver,
            shared,
            stop,
            thread: Some(thread),
            close_requested: false,
        })
    }

    pub(crate) fn take_close_request(&mut self) -> bool {
        if !self.shared.alive() && !self.close_requested {
            self.close_requested = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn dispatch(&mut self, editor: &mut Editor, context: &egui::Context) {
        for _ in 0..UI_REQUESTS_PER_FRAME {
            if !self.shared.alive() {
                break;
            }
            let Ok(envelope) = self.receiver.try_recv() else {
                break;
            };
            if envelope.reply.is_closed()
                || envelope.cancellation.is_cancelled()
                || !self.shared.alive()
                || Instant::now() >= envelope.deadline
            {
                continue;
            }
            let may_install = || {
                #[cfg(test)]
                if let Some(probe) = &envelope.install_probe {
                    probe();
                }
                !envelope.reply.is_closed()
                    && !envelope.cancellation.is_cancelled()
                    && self.shared.alive()
                    && Instant::now() < envelope.deadline
            };
            let result = editor.remote_operation_guarded(envelope.request, context, &may_install);
            let changed = matches!(
                &result,
                Ok(OperationReply::Created { .. }
                    | OperationReply::Changed
                    | OperationReply::DocumentApplied { .. }
                    | OperationReply::PendingConfirmation { .. }
                    | OperationReply::Cancelled { .. }
                    | OperationReply::CloseRequested { .. })
                    | Ok(OperationReply::Saved { .. }
                        | OperationReply::SaveUncertain { .. }
                        | OperationReply::SaveReconciled { .. })
            );
            if matches!(&result, Ok(OperationReply::CloseRequested { .. })) {
                self.shared
                    .arm_close(envelope.request_id.clone(), &envelope.reply);
            }
            if envelope.reply.send(result).is_err() {
                self.shared.fail_close(&envelope.request_id);
            }
            if changed {
                context.request_repaint();
            }
        }
        if self.shared.alive() && !self.receiver.is_empty() {
            context.request_repaint();
        }
    }
}

impl Drop for UiBridge {
    fn drop(&mut self) {
        self.shared.close();
        self.stop.cancel();
        if let Some(thread) = self.thread.take()
            && thread.is_finished()
            && thread.join().is_err()
        {
            eprintln!("MCP runtime thread failed");
        }
    }
}

#[cfg(test)]
pub(crate) struct TestIngress {
    sender: mpsc::Sender<EditorEnvelope>,
    shared: Arc<Shared>,
}

#[cfg(test)]
impl TestIngress {
    pub(crate) fn queue(
        &self,
        request: RemoteRequest,
    ) -> Result<oneshot::Receiver<Result<OperationReply, OperationError>>, ()> {
        self.queue_with_save_probe(
            request,
            CancellationToken::new(),
            Instant::now() + UI_REPLY_TIMEOUT,
            None,
        )
    }

    pub(crate) fn queue_with_save_probe(
        &self,
        request: RemoteRequest,
        cancellation: CancellationToken,
        deadline: Instant,
        install_probe: Option<Box<dyn Fn() + Send>>,
    ) -> Result<oneshot::Receiver<Result<OperationReply, OperationError>>, ()> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(EditorEnvelope {
                request,
                reply,
                cancellation,
                request_id: RequestId::Number(-1),
                deadline,
                install_probe,
            })
            .map_err(|_| ())?;
        Ok(receiver)
    }

    pub(crate) fn close(&self) {
        self.shared.close();
    }

    pub(crate) fn force_close_delivery(&self, delivery: CloseDelivery) {
        self.shared.test_force_close_delivery(delivery);
    }
}

#[cfg(test)]
pub(crate) fn test_bridge(context: egui::Context) -> (UiBridge, TestIngress) {
    let (sender, receiver) = mpsc::channel(UI_QUEUE_CAPACITY);
    let shared = Arc::new(Shared::new(context));
    let bridge = UiBridge {
        receiver,
        shared: shared.clone(),
        stop: CancellationToken::new(),
        thread: None,
        close_requested: false,
    };
    (bridge, TestIngress { sender, shared })
}

#[cfg(all(test, feature = "mcp-test-client"))]
pub(crate) fn test_connection(
    context: egui::Context,
) -> (
    UiBridge,
    tokio::io::DuplexStream,
    impl Future<Output = ()> + Send,
) {
    test_connection_writer(context, |writer| writer)
}

#[cfg(all(unix, test, feature = "mcp-test-client"))]
pub(crate) struct TestFlushFailure {
    armed: std::sync::atomic::AtomicBool,
    failed: std::sync::atomic::AtomicBool,
    accepted: std::sync::atomic::AtomicUsize,
}

#[cfg(all(unix, test, feature = "mcp-test-client"))]
impl TestFlushFailure {
    pub(crate) fn arm(&self) {
        use std::sync::atomic::Ordering;
        self.accepted.store(0, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }
    pub(crate) fn observed(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.failed.load(Ordering::SeqCst) && self.accepted.load(Ordering::SeqCst) > 0
    }
}

#[cfg(all(unix, test, feature = "mcp-test-client"))]
struct TestFailingWriter {
    inner: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    failure: Arc<TestFlushFailure>,
}

#[cfg(all(unix, test, feature = "mcp-test-client"))]
impl tokio::io::AsyncWrite for TestFailingWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        use std::sync::atomic::Ordering;
        let result =
            tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(&mut self.inner), context, bytes);
        if let std::task::Poll::Ready(Ok(count)) = &result
            && self.failure.armed.load(Ordering::SeqCst)
        {
            self.failure.accepted.fetch_add(*count, Ordering::SeqCst);
        }
        result
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::sync::atomic::Ordering;
        if self.failure.armed.swap(false, Ordering::SeqCst) {
            self.failure.failed.store(true, Ordering::SeqCst);
            return std::task::Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.inner), context)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(std::pin::Pin::new(&mut self.inner), context)
    }
}

#[cfg(all(unix, test, feature = "mcp-test-client"))]
pub(crate) fn test_connection_with_flush_failure(
    context: egui::Context,
) -> (
    UiBridge,
    tokio::io::DuplexStream,
    impl Future<Output = ()> + Send,
    Arc<TestFlushFailure>,
) {
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    let failure = Arc::new(TestFlushFailure {
        armed: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        accepted: AtomicUsize::new(0),
    });
    let captured = failure.clone();
    let (bridge, client, serving) = test_connection_writer(context, |inner| TestFailingWriter {
        inner,
        failure: captured,
    });
    (bridge, client, serving, failure)
}

#[cfg(all(test, feature = "mcp-test-client"))]
fn test_connection_writer<W>(
    context: egui::Context,
    wrap: impl FnOnce(tokio::io::WriteHalf<tokio::io::DuplexStream>) -> W,
) -> (
    UiBridge,
    tokio::io::DuplexStream,
    impl Future<Output = ()> + Send,
)
where
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let (bridge, ingress) = test_bridge(context);
    let (server_io, client_io) = tokio::io::duplex(transport::MAX_EGRESS_BYTES);
    let (reader, writer) = tokio::io::split(server_io);
    let server = tools::EditorServer::new(ingress.sender, ingress.shared.clone());
    let transport = BoundedTransport::new(reader, wrap(writer), ingress.shared);
    let serving = async {
        server
            .serve(transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    };
    (bridge, client_io, serving)
}
