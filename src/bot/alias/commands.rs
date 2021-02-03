use {
    crate::{db::MessageAliasDatabase, Synced},
    anyhow::{Context as _, Result},
};

#[rustfmt::skip]
pub(super) fn help() -> String {
r#"```asciidoc
= rusty_ponyo::alias =
g!alias [subcommand] [args...]

>>> 引数において " は省略できません <<<

= subcommands =
    help                         :: この文を出します
    make "[キー]" "[メッセージ]" :: エイリアスを作成します
    delete "[キー]"              :: エイリアスを削除します
```"#.into()
}

const KEY_LENGTH_LIMIT: usize = 100;
const MSG_LENGTH_LIMIT: usize = 500;

pub(super) async fn make(
    db: &Synced<impl MessageAliasDatabase>,
    key: &str,
    msg: &str,
) -> Result<String> {
    let mut error_msgs = vec![];

    let key_len = key.chars().count();
    let msg_len = msg.chars().count();

    if key_len > KEY_LENGTH_LIMIT {
        error_msgs.push(format!(
            "長すぎるキー({}文字)です。{}文字以下にしてください。",
            key_len, KEY_LENGTH_LIMIT
        ));
    }

    if msg_len > MSG_LENGTH_LIMIT {
        error_msgs.push(format!(
            "長すぎるメッセージ({}文字)です。{}文字以下にしてください。",
            msg_len, MSG_LENGTH_LIMIT
        ));
    }

    if !error_msgs.is_empty() {
        return Ok(error_msgs.join("\n"));
    }

    db.write()
        .await
        .save(key, msg)
        .await
        .context("failed to save new alias")?;

    Ok("作成しました".into())
}

pub(super) async fn delete(db: &Synced<impl MessageAliasDatabase>, key: &str) -> Result<String> {
    let deleted = db
        .write()
        .await
        .delete(key)
        .await
        .context("failed to delete alias")?;

    if deleted {
        Ok("削除しました".into())
    } else {
        Ok("そのようなキーを持つエイリアスはありません".into())
    }
}
