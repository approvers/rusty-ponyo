use {
    crate::{
        bot::{Attachment, BotService, Message},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
    std::io::{stdin, stdout, Write},
};

pub(crate) struct ConsoleClient {
    services: Vec<Box<dyn ServiceEntry>>,
}

impl ConsoleClient {
    pub fn new() -> Self {
        Self { services: vec![] }
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

    pub async fn run(self) -> Result<()> {
        let mut buf = String::new();

        loop {
            tokio::task::block_in_place(|| {
                print!("> ");
                stdout().flush().unwrap();
                stdin().read_line(&mut buf).unwrap();
            });

            let message = ConsoleMessage { content: buf };

            for service in &self.services {
                match service.on_message(&message).await {
                    Ok(Some(t)) => println!("{}", t),
                    Err(e) => println!("{:?}", e),
                    _ => {}
                };
            }

            buf = message.content;
            buf.clear();
        }
    }
}

struct ConsoleMessage {
    content: String,
}

impl Message for ConsoleMessage {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &[] // TODO: support this
    }
}

struct ConsoleAttachment {
    name: String,
    data: Vec<u8>,
}

#[async_trait]
impl Attachment for ConsoleAttachment {
    fn name(&self) -> &str {
        &self.name
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }
}
