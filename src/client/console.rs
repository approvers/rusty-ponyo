use {
    crate::{
        bot::{Attachment, BotService, Context, Message, SendMessage, User},
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

            let message = {
                if input.starts_with(ATTACHMENT_CMD) {
                    for a in input[ATTACHMENT_CMD.len()..].trim().split(" ") {
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

                    ConsoleMessage {
                        content,
                        attachments: attachments.iter().map(|x| x as _).collect::<Vec<_>>(),
                    }
                } else {
                    ConsoleMessage {
                        content: input,
                        attachments: vec![],
                    }
                }
            };

            for service in &self.services {
                let ctx = ConsoleContext {
                    service_name: service.name(),
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
}

impl Message for ConsoleMessage<'_> {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &self.attachments
    }

    fn author(&self) -> &dyn crate::bot::User {
        &ConsoleUser
    }
}

struct ConsoleUser;

impl User for ConsoleUser {
    fn id(&self) -> u64 {
        0
    }

    fn name(&self) -> &str {
        "ConsoleUser"
    }
}

struct ConsoleAttachment<'a> {
    name: &'a str,
}

#[async_trait]
impl Attachment for ConsoleAttachment<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}

struct ConsoleContext {
    service_name: &'static str,
}

#[async_trait]
impl Context for ConsoleContext {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        println!("({}): {}", self.service_name, msg.content);

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
