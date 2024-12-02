use {
    crate::bot::{
        parse_command, ui, BotService, Context, IsUpdated, Message, KAWAEMON_DISCORD_USER_ID,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    clap::{ArgGroup, ValueEnum},
    model::{Meigen, MeigenId},
};

pub mod model;

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Id,
    Love,
    Length,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[clap(alias = "a")]
    #[default]
    Asc,
    #[clap(alias = "d")]
    Desc,
}

#[derive(Default)]
pub struct FindOptions<'a> {
    pub author: Option<&'a str>,
    pub content: Option<&'a str>,
    pub offset: u32,
    pub limit: u8,
    pub sort: SortKey,
    pub dir: SortDirection,
    pub random: bool,
}

#[async_trait]
pub trait MeigenDatabase: Send + Sync {
    async fn save(
        &self,
        author: impl Into<String> + Send,
        content: impl Into<String> + Send,
    ) -> Result<Meigen>;
    async fn load(&self, id: MeigenId) -> Result<Option<Meigen>>;
    async fn delete(&self, id: MeigenId) -> Result<IsUpdated>;
    async fn search(&self, options: FindOptions<'_>) -> Result<Vec<Meigen>>;
    async fn count(&self) -> Result<u32>;
    async fn append_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated>;
    async fn remove_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated>;
}

const NAME: &str = "rusty_ponyo::bot::meigen";
const PREFIX: &str = "g!meigen";
const MEIGEN_LENGTH_LIMIT: usize = 300;
const LIST_LENGTH_LIMIT: usize = 500;

ui! {
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// 名言を新規登録します
    Make {
        /// 名言を発言した人の名前
        author: String,

        /// 名言の内容
        content: String,
    },

    /// 指定されたIDを持つ名言を表示します
    Show {
        /// 名言のID
        id: MeigenId,

        /// 指定すると名言をGopherのASCIIアートで表示します
        #[clap(long)]
        gopher: bool,

        /// 指定すると名言をFerrisのASCIIアートで表示します
        #[clap(long)]
        ferris: bool,
    },

    /// 現在登録されている名言の数を表示します
    Status,

    /// 名言をリスト表示します
    #[clap(group(
        ArgGroup::new("dir_conflict")
            .args(&["dir"])
            .requires("dir")
            .conflicts_with("reverse")
    ))] // --dir and --reverse conflicts
    List {
        /// 表示する名言のオフセット
        #[clap(long)]
        #[clap(default_value_t = 0)]
        offset: u32,

        /// 表示する名言の数
        #[clap(long)]
        #[clap(default_value_t = 5)]
        #[clap(value_parser(clap::value_parser!(u8).range(1..=10)))]
        limit: u8,

        /// 指定された場合、検索条件に合致する名言の中からランダムに選び出して出力します
        #[clap(short, long)]
        #[clap(default_value_t = false)]
        random: bool,

        /// 指定した人の名言をリスト表示します
        #[clap(long)]
        author: Option<String>,

        /// 指定した文字列を含む名言をリスト表示します
        #[clap(long)]
        content: Option<String>,

        /// 指定した項目でソートします。
        #[clap(value_enum, long, default_value_t)]
        sort: SortKey,

        /// ソートの順番を入れ替えます。
        #[clap(value_enum, long, default_value_t)]
        dir: SortDirection,

        /// 降順にします。--dir desc のエイリアスです。
        #[clap(short = 'R', long, alias = "rev")]
        #[clap(default_value_t = false)]
        reverse: bool,
    },

    /// 名言を削除します
    /// かわえもんにしか使えません
    Delete { id: MeigenId },

    /// 名言にいいねをします
    Love { id: MeigenId },

    /// 名言のいいねを取り消します
    Unlove { id: MeigenId },
}

pub struct MeigenBot<D> {
    db: D,
}

#[async_trait]
impl<D: MeigenDatabase> BotService for MeigenBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await? else {
            return Ok(());
        };

        let res = match parsed.command {
            Command::Make { author, content } => self.make(author, content).await?,
            Command::Show { id, gopher, ferris } => {
                if gopher {
                    self.gophersay(id).await?
                } else if ferris {
                    self.ferrissay(id).await?
                } else {
                    self.show(id).await?
                }
            }
            Command::Status => self.status().await?,
            Command::List {
                offset,
                limit,
                random,
                author,
                content,
                sort,
                dir,
                reverse,
            } => {
                self.search(FindOptions {
                    author: author.as_deref(),
                    content: content.as_deref(),
                    offset,
                    limit,
                    sort,
                    dir: if reverse { SortDirection::Desc } else { dir },
                    random,
                })
                .await?
            }
            Command::Delete { id } => self.delete(msg.author().id(), id).await?,
            Command::Love { id } => self.love(msg.author().id(), id).await?,
            Command::Unlove { id } => self.unlove(msg.author().id(), id).await?,
        };

        ctx.send_text_message(&res).await?;

        Ok(())
    }
}

impl<D: MeigenDatabase> MeigenBot<D> {
    pub fn new(db: D) -> Self {
        Self { db }
    }

    async fn make(&self, author: String, content: String) -> Result<String> {
        let strip = |s: &str| s.trim().replace('`', "");

        let author = strip(&author);
        let content = strip(&content);

        let len = author.chars().count() + content.chars().count();
        if len > MEIGEN_LENGTH_LIMIT {
            return Ok(format!(
                "名言が長すぎます({len}文字)。{MEIGEN_LENGTH_LIMIT}文字以下にしてください。"
            ));
        }

        let meigen = self.db.save(author, content).await?;
        Ok(meigen.to_string())
    }

    async fn show(&self, id: MeigenId) -> Result<String> {
        Ok(match self.db.load(id).await? {
            Some(x) => x.to_string(),
            None => format!("No.{id} を持つ名言は見つかりませんでした。"),
        })
    }

    async fn status(&self) -> Result<String> {
        let count = self
            .db
            .count()
            .await
            .context("Failed to fetch meigen count")?;

        Ok(format!("```\n現在登録されている名言数: {count}\n```"))
    }

    async fn search(&self, opt: FindOptions<'_>) -> Result<String> {
        let res = self.db.search(opt).await?;
        if res.is_empty() {
            return Ok("条件に合致する名言が見つかりませんでした".into());
        }

        Ok(list(&res))
    }

    async fn delete(&self, caller: u64, id: MeigenId) -> Result<String> {
        if caller != KAWAEMON_DISCORD_USER_ID {
            return Ok("名言削除はかわえもんにしか出来ません".into());
        }

        Ok(if self.db.delete(id).await? {
            "削除しました".into()
        } else {
            format!("No.{id} を持つ名言は見つかりませんでした。")
        })
    }

    async fn gophersay(&self, id: MeigenId) -> Result<String> {
        let meigen = self.db.load(id).await.context("failed to get meigen")?;

        Ok(match meigen {
            Some(meigen) => format_ascii_meigen(&meigen, include_str!("./gopher.ascii")),
            None => format!("No.{id} を持つ名言は見つかりませんでした。"),
        })
    }

    async fn ferrissay(&self, id: MeigenId) -> Result<String> {
        let meigen = self.db.load(id).await.context("failed to get meigen")?;

        Ok(match meigen {
            Some(meigen) => format_ascii_meigen(&meigen, include_str!("./ferris.ascii")),
            None => format!("No.{id} を持つ名言は見つかりませんでした。"),
        })
    }

    async fn love(&self, caller: u64, id: MeigenId) -> Result<String> {
        if self.db.load(id).await?.is_none() {
            return Ok(format!("No.{id} を持つ名言は見つかりませんでした。"));
        }

        Ok(if self.db.append_loved_user(id, caller).await? {
            "いいねしました".to_string()
        } else {
            "すでにいいねしています".to_string()
        })
    }

    async fn unlove(&self, caller: u64, id: MeigenId) -> Result<String> {
        if self.db.load(id).await?.is_none() {
            return Ok(format!("No.{id} を持つ名言は見つかりませんでした。"));
        }

        Ok(if self.db.remove_loved_user(id, caller).await? {
            "いいねを解除しました".to_string()
        } else {
            "いいねしていません".to_string()
        })
    }
}

fn list(meigens: &[Meigen]) -> String {
    let mut res = String::new();
    let mut chars = 0;

    for meigen in meigens {
        if chars > LIST_LENGTH_LIMIT {
            res.insert_str(0, "結果が長すぎたため一部の名言は省略されました\n");
            break;
        }

        let meigen = meigen.to_string();
        chars += meigen.chars().count();
        res += &meigen;
        res += "\n";
    }

    res.trim().to_string()
}

fn format_ascii_meigen(meigen: &Meigen, ascii_art: &str) -> String {
    let meigen = format!("{}\n  --- {}", meigen.content, meigen.author)
        .lines()
        .collect::<Vec<_>>()
        .join("\n  ");

    let bar_length = meigen
        .lines()
        .map(|x| {
            x.chars()
                .map(|x| if x.is_ascii() { 1 } else { 2 })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(30)
        + 4;

    let bar = "-".chars().cycle().take(bar_length).collect::<String>();

    format!("```\n{bar}\n   {meigen}\n{bar}\n{}\n```", ascii_art)
}

#[test]
fn test_format_gopher() {
    assert_eq!(
        format_ascii_meigen(&Meigen {
            id: MeigenId(1),
            author: "あいうえお".to_string(),
            content: "abcdeあいうえおdddあ".to_string(),
            loved_user_id: vec![],
        }, include_str!("./gopher.ascii")),
        "```\n------------------------\n   abcdeあいうえおdddあ\n    --- あいうえお\n------------------------\n    \\\n     \\\n      \\\n         ,_---~~~~~----._         \n  _,,_,*^____      _____``*g*\\\"*, \n / __/ /'     ^.  /      \\ ^@q   f \n[  @f | @))    |  | @))   l  0 _/  \n \\`/   \\~____ / __ \\_____/    \\   \n  |           _l__l_           I   \n  }          [______]           I  \n  ]            | | |            |  \n  ]             ~ ~             |  \n  |                            |   \n   |                           |   \n\n```"
    );
}

#[test]
fn test_format_ferris() {
    assert_eq!(
        format_ascii_meigen(&Meigen {
            id: MeigenId(1),
            author: "あいうえお".to_string(),
            content: "abcdeあいうえおdddあ".to_string(),
            loved_user_id: vec![],
        }, include_str!("./ferris.ascii")),
        "```\n------------------------\n   abcdeあいうえおdddあ\n    --- あいうえお\n------------------------\n       \\\n        \\\n         \\\n            _~^~^~_\n        \\) /  o o  \\ (/\n          '_   -   _'\n          / '-----' \\\n\n```"
    );
}
