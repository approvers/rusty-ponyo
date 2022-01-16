use {
    crate::{
        bot::{Attachment, BotService, Context, Message, SendMessage, User},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
    std::{
        io::{stdin, stdout, Write},
        time::Instant,
    },
};

pub(crate) struct ConsoleClient {
    services: Vec<Box<dyn ServiceEntry>>,
}

impl ConsoleClient {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn add_service<S, D>(&mut self, service: S, db: Synced<D>) -> &mut Self
    where
        S: BotService<Database = D> + 'static,
        D: ThreadSafe + 'static,
    {
        self.services
            .push(Box::new(ServiceEntryInner { service, db }));
        self
    }

    pub async fn run(self) -> Result<()> {
        let read_line = || {
            tokio::task::block_in_place(|| {
                let mut buf = String::new();

                println!();
                print!("> ");
                stdout().flush().unwrap();
                stdin().read_line(&mut buf).unwrap();

                buf.trim().to_string()
            })
        };

        loop {
            let input = read_line();
            let mut attachments = vec![];

            const ATTACHMENT_CMD: &str = "!attachments";

            let (content, attachments) = {
                if let Some(stripped) = input.strip_prefix(ATTACHMENT_CMD) {
                    for a in stripped.trim().split(' ') {
                        attachments.push(ConsoleAttachment { name: a.trim() });
                    }

                    println!(
                        "(ConsoleClient): {} attachments confirmed. type message. or type STOP to cancel.",
                        attachments.len()
                    );

                    let content = read_line();

                    if content == "STOP" {
                        println!("(ConsoleClient): canceled.");
                        continue;
                    }

                    (
                        content,
                        attachments.iter().map(|x| x as _).collect::<Vec<_>>(),
                    )
                } else {
                    (input, vec![])
                }
            };

            for service in &self.services {
                let begin = Instant::now();

                let ctx = ConsoleContext {
                    service_name: service.name(),
                    begin,
                };

                let message = ConsoleMessage {
                    content: content.clone(),
                    attachments: attachments.clone(),
                    user: ConsoleUser {
                        service_name: service.name(),
                        begin,
                    },
                };

                let result = service.on_message(&message, &ctx).await;

                if let Err(e) = result {
                    println!(
                        "(ConsoleClient): error occur while calling service: {:?}",
                        e
                    );
                }
            }
        }
    }
}

struct ConsoleMessage<'a> {
    content: String,
    attachments: Vec<&'a dyn Attachment>,
    user: ConsoleUser<'a>,
}

impl Message for ConsoleMessage<'_> {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &self.attachments
    }

    fn author(&self) -> &dyn crate::bot::User {
        &self.user
    }
}

struct ConsoleUser<'a> {
    service_name: &'a str,
    begin: Instant,
}

#[async_trait]
impl<'a> User for ConsoleUser<'a> {
    fn id(&self) -> u64 {
        0
    }

    fn name(&self) -> &str {
        "ConsoleUser"
    }

    async fn dm(&self, msg: SendMessage<'_>) -> Result<()> {
        println!(
            "({}, DM, {}ms): {}",
            self.service_name,
            self.begin.elapsed().as_millis(),
            msg.content
        );

        Ok(())
    }
}

struct ConsoleAttachment<'a> {
    name: &'a str,
}

#[async_trait]
impl Attachment for ConsoleAttachment<'_> {
    fn name(&self) -> &str {
        self.name
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}

struct ConsoleContext {
    service_name: &'static str,
    begin: Instant,
}

#[async_trait]
impl Context for ConsoleContext {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        println!(
            "({}, {}ms): {}",
            self.service_name,
            self.begin.elapsed().as_millis(),
            msg.content
        );

        if !msg.attachments.is_empty() {
            println!(
                "with {} attachments: {}",
                msg.attachments.len(),
                msg.attachments
                    .iter()
                    .map(|x| x.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }

        Ok(())
    }

    async fn get_user_name(&self, _user_id: u64) -> Result<String> {
        Ok("ConsoleUser".to_string())
    }
}
