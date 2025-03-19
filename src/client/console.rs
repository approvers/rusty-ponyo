use {
    crate::{
        bot::{Attachment, BotService, Context, Message, Runtime, SendMessage, User},
        client::{ListCons, ListNil, ServiceList, ServiceVisitor},
    },
    anyhow::Result,
    std::{
        io::{Write, stdin, stdout},
        path::Path,
        time::Instant,
    },
};

pub struct ConsoleClient<L: ServiceList<ConsoleRuntime>> {
    services: L,
}

impl ConsoleClient<ListNil> {
    pub fn new() -> Self {
        Self { services: ListNil }
    }
}

impl<L: ServiceList<ConsoleRuntime>> ConsoleClient<L> {
    pub fn add_service<S>(self, service: S) -> ConsoleClient<ListCons<ConsoleRuntime, S, L>>
    where
        S: BotService<ConsoleRuntime> + Send,
    {
        ConsoleClient {
            services: self.services.append(service),
        }
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

                    (content, attachments)
                } else {
                    (input, vec![])
                }
            };

            struct Visitor {
                content: String,
                attachments: Vec<ConsoleAttachment>,
            }
            impl ServiceVisitor<ConsoleRuntime> for Visitor {
                async fn visit(&self, service: &impl BotService<ConsoleRuntime>) {
                    let begin = Instant::now();

                    let ctx = ConsoleContext {
                        service_name: service.name(),
                        begin,
                    };

                    let message = ConsoleMessage {
                        service_name: service.name().to_owned(),
                        begin,
                        content: self.content.clone(),
                        attachments: self.attachments.clone(),
                        user: ConsoleUser {
                            service_name: service.name().to_owned(),
                            begin,
                        },
                    };

                    let result = service.on_message(&message, &ctx).await;

                    if let Err(e) = result {
                        println!("(ConsoleClient): error while calling service: {e:?}",);
                    }
                }
            }

            self.services
                .visit(&Visitor {
                    content,
                    attachments,
                })
                .await;
        }
    }
}

pub struct ConsoleRuntime;
impl Runtime for ConsoleRuntime {
    type Message = ConsoleMessage;
    type Context = ConsoleContext;
}

pub struct ConsoleMessage {
    service_name: String,
    begin: Instant,
    content: String,
    attachments: Vec<ConsoleAttachment>,
    user: ConsoleUser,
}

impl Message for ConsoleMessage {
    type Attachment = ConsoleAttachment;
    type User = ConsoleUser;

    async fn reply(&self, content: &str) -> Result<()> {
        println!(
            "({}, reply, {}ms): {}",
            self.service_name,
            self.begin.elapsed().as_millis(),
            content
        );
        Ok(())
    }

    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[ConsoleAttachment] {
        &self.attachments
    }

    fn author(&self) -> &ConsoleUser {
        &self.user
    }
}

pub struct ConsoleUser {
    service_name: String,
    begin: Instant,
}

impl User for ConsoleUser {
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

#[derive(Clone)]
pub struct ConsoleAttachment {
    path: String,
    content: Vec<u8>,
}

impl ConsoleAttachment {
    fn load(path: &str) -> Result<Self, std::io::Error> {
        let content = std::fs::read(path)?;
        Ok(ConsoleAttachment {
            content,
            path: path.to_owned(),
        })
    }
}

impl Attachment for ConsoleAttachment {
    fn name(&self) -> &str {
        Path::new(&self.path).file_name().unwrap().to_str().unwrap()
    }

    fn size(&self) -> usize {
        self.content.len()
    }

    async fn download(&self) -> Result<Vec<u8>> {
        Ok(self.content.clone())
    }
}

pub struct ConsoleContext {
    service_name: &'static str,
    begin: Instant,
}

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

    async fn is_bot(&self, _user_id: u64) -> Result<bool> {
        Ok(false)
    }

    async fn get_user_name(&self, _user_id: u64) -> Result<String> {
        Ok("ConsoleUser".to_string())
    }
}
