use {
    crate::{
        bot::BotService,
        client::{Message, ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::Result,
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
        S: BotService<Database = D>,
        D: ThreadSafe,
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

            buf = String::new();
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
}
