use {
    crate::bot::{Attachment, BotService, Context, Message, SendMessage, User},
    anyhow::Result,
    async_trait::async_trait,
    std::{
        io::{stdin, stdout, Write},
        path::Path,
        time::Instant,
    },
};

pub(crate) struct ConsoleClient<'a> {
    services: Vec<Box<dyn BotService + 'a>>,
}

impl<'a> ConsoleClient<'a> {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn add_service<S>(&mut self, service: S) -> &mut Self
    where
        S: BotService + Send + 'a,
    {
        self.services.push(Box::new(service));
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

            const ATTACHMENT_CMD: &str = "!attachment";

            let (content, attachments) = {
                if let Some(stripped) = input.strip_prefix(ATTACHMENT_CMD) {
                    match ConsoleAttachment::load(stripped.trim()) {
                        Ok(a) => attachments.push(a),
                        Err(e) => {
                            println!("(ConsoleClient): failed to load attachment: {e}");
                            continue;
                        }
                    };

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

            for service in self.services.iter() {
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
                    println!("(ConsoleClient): error while calling service: {e:?}",);
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
    path: &'a str,
    content: Vec<u8>,
}

impl<'a> ConsoleAttachment<'a> {
    fn load(path: &'a str) -> Result<Self, std::io::Error> {
        let content = std::fs::read(path)?;
        Ok(ConsoleAttachment { content, path })
    }
}

#[async_trait]
impl<'a> Attachment for ConsoleAttachment<'a> {
    fn name(&self) -> &str {
        Path::new(self.path).file_name().unwrap().to_str().unwrap()
    }

    fn size(&self) -> usize {
        self.content.len()
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(self.content.clone())
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
                    .map(|x| format!(
                        "{} ({:.2}MiB)",
                        x.name,
                        (x.data.len() as f64 / (1024.0 * 1024.0))
                    ))
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
