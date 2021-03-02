use std::io::{BufRead, BufReader, BufWriter};
use std::sync::Arc;
use tokio::sync::RwLock;
use {
    crate::{
        bot::{Attachment, BotService, Context, Message, SendMessage},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
    std::io::{Read, Write},
};

pub(crate) struct BufferClient<R, W>
where
    R: Read,
    W: Write + ThreadSafe,
{
    reader: BufReader<R>,
    writer: Synced<BufWriter<W>>,
    services: Vec<Box<dyn ServiceEntry>>,
}

impl<R, W> BufferClient<R, W>
where
    R: Read,
    W: Write + ThreadSafe,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer: Arc::new(RwLock::new(BufWriter::new(writer))),
            services: vec![],
        }
    }

    pub fn add_service<S, D>(mut self, service: S, db: Synced<D>) -> Self
    where
        S: BotService<Database = D> + 'static,
        D: ThreadSafe + 'static,
    {
        self.services
            .push(Box::new(ServiceEntryInner { service, db }));
        self
    }

    async fn write_line<S: Into<String>>(&self, message: S) {
        writeln!(self.writer.write().await, "{}", message.into()).unwrap();
    }
    async fn read_line(&mut self) -> String {
        self.write_line("").await;
        self.write_line("> ").await;
        self.writer.write().await.flush().unwrap();

        tokio::task::block_in_place(|| {
            let mut buf = String::new();
            self.reader.read_line(&mut buf).unwrap();

            buf.trim().to_string()
        })
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            let input = self.read_line().await;
            let mut attachments = vec![];

            const ATTACHMENT_CMD: &str = "!attachments";

            let message = {
                if input.starts_with(ATTACHMENT_CMD) {
                    for a in input[ATTACHMENT_CMD.len()..].trim().split(" ") {
                        attachments.push(BufferAttachment { name: a.trim() });
                    }

                    self.write_line(format!(
                        "(ConsoleClient): {} attachments confirmed. type message. or type STOP to cancel.",
                        attachments.len()
                    )).await;

                    let content = self.read_line().await;

                    if content == "STOP" {
                        self.write_line("(ConsoleClient): canceled.").await;
                        continue;
                    }

                    BufferMessage {
                        content,
                        attachments: attachments.iter().map(|x| x as _).collect::<Vec<_>>(),
                    }
                } else {
                    BufferMessage {
                        content: input,
                        attachments: vec![],
                    }
                }
            };

            for service in &self.services {
                let ctx = BufferContext {
                    writer: self.writer.clone(),
                    service_name: service.name(),
                };

                let result = service.on_message(&message, &ctx).await;
                if let Err(e) = result {
                    self.write_line(format!(
                        "(ConsoleClient): error occur while calling service: {:?}",
                        e,
                    ))
                    .await;
                }
            }
        }
    }
}

struct BufferMessage<'a> {
    content: String,
    attachments: Vec<&'a dyn Attachment>,
}

impl Message for BufferMessage<'_> {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &self.attachments
    }
}

struct BufferAttachment<'a> {
    name: &'a str,
}

#[async_trait]
impl Attachment for BufferAttachment<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}

struct BufferContext<W>
where
    W: Write + ThreadSafe,
{
    writer: Synced<BufWriter<W>>,
    service_name: &'static str,
}

impl<W> BufferContext<W>
where
    W: Write + ThreadSafe,
{
    async fn write_line<S: Into<String>>(&self, message: S) {
        writeln!(self.writer.write().await, "{}", message.into()).unwrap();
    }
}

#[async_trait]
impl<W> Context for BufferContext<W>
where
    W: Write + ThreadSafe,
{
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        self.write_line(format!("({}): {}", self.service_name, msg.content))
            .await;

        if !msg.attachments.is_empty() {
            self.write_line(format!(
                "with {} attachments: {}",
                msg.attachments.len(),
                msg.attachments
                    .iter()
                    .map(|x| x.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .await;
        }

        Ok(())
    }
}

mod test {
    mod buffer_client {
        use crate::bot::{BotService, Context, Message};
        use crate::client::console::BufferClient;
        use crate::Synced;
        use anyhow::Result;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use std::io::{stdin, stdout};
        use async_trait::async_trait;

        #[test]
        fn add_service() {
            #[derive(PartialEq)]
            struct MockService;
            #[async_trait]
            impl BotService for MockService {
                const NAME: &'static str = "mock";
                type Database = ();

                async fn on_message(
                    &self,
                    _: &Synced<Self::Database>,
                    _: &dyn Message,
                    _: &dyn Context,
                ) -> Result<()> {
                    unimplemented!()
                }
            }

            let db = Arc::new(RwLock::new(()));

            let client = BufferClient::new(stdin(), stdout())
                .add_service(MockService, db.clone());

            assert!(client.services.iter().any(|x| { x.name() == "mock" }))
        }
    }
}
