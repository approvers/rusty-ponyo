use {
    crate::bot::{Attachment, BotService, Context, Message, Runtime, SendMessage, User},
    anyhow::Result,
    pretty_assertions::*,
    std::sync::Mutex,
};

pub struct TestRuntime;
impl Runtime for TestRuntime {
    type Message = TestMessage;
    type Context = TestContext;
}

pub struct TestUser;
impl User for TestUser {
    fn id(&self) -> u64 {
        todo!()
    }
    fn name(&self) -> &str {
        todo!()
    }
    async fn dm(&self, _msg: SendMessage<'_>) -> Result<()> {
        todo!()
    }
}
pub struct TestAttachment;
impl Attachment for TestAttachment {
    fn name(&self) -> &str {
        todo!()
    }
    fn size(&self) -> usize {
        todo!()
    }
    async fn download(&self) -> Result<Vec<u8>> {
        todo!()
    }
}

pub struct TestContext {
    pub msg: Mutex<Vec<String>>,
}
impl Context for TestContext {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        assert!(
            msg.attachments.is_empty(),
            "todo: attachments not supported"
        );
        self.msg.lock().unwrap().push(msg.content.to_owned());
        Ok(())
    }
    async fn get_user_name(&self, _user_id: u64) -> Result<String> {
        todo!()
    }
    async fn is_bot(&self, _user_id: u64) -> Result<bool> {
        todo!()
    }
}
pub struct TestMessage {
    pub author: TestUser,
    pub content: String,
}
impl Message for TestMessage {
    type Attachment = TestAttachment;
    type User = TestUser;

    async fn reply(&self, _msg: &str) -> Result<()> {
        todo!()
    }
    fn author(&self) -> &Self::User {
        &self.author
    }
    fn content(&self) -> &str {
        &self.content
    }
    fn attachments(&self) -> &[Self::Attachment] {
        todo!()
    }
}

pub async fn run(service: impl BotService<TestRuntime>, snapshot: &str) {
    let mut lines = snapshot.lines().peekable();
    while let Some(input) = lines.next() {
        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let Some(input) = input.strip_prefix("> ") else {
            panic!("should be start with '> ': {input}")
        };

        let msg = TestMessage {
            author: TestUser,
            content: input.trim().to_owned(),
        };

        let ctx = TestContext {
            msg: Mutex::new(vec![]),
        };

        service.on_message(&msg, &ctx).await.unwrap();

        let mut expected_output = vec![];
        while matches!(lines.peek(), Some(s) if !s.starts_with("> ")) {
            expected_output.push(lines.next().unwrap())
        }

        let expected_output = expected_output.join("\n");
        let expected_output = expected_output.trim();

        let actual_contents = ctx.msg.into_inner().unwrap();

        self::assert_eq!(
            actual_contents.len(),
            1,
            "handling for multiple/none output messages not implemented"
        );

        self::assert_eq!(expected_output, actual_contents[0]);
    }
}
