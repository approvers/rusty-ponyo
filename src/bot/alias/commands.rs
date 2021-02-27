use chrono::Utc;

use {
    crate::{
        bot::Attachment,
        db::MessageAliasDatabase,
        model::{MessageAlias, MessageAliasAttachment},
        Synced,
    },
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
const ATTACHMENTS_MAX_COUNT: usize = 1;

pub(super) async fn make(
    db: &Synced<impl MessageAliasDatabase>,
    key: &str,
    msg: &str,
    attachments: &[&dyn Attachment],
) -> Result<String> {
    if db.read().await.get(key).await?.is_some() {
        return Ok("すでにそのキーにはエイリアスが登録されています。上書きしたい場合は先に削除してください。".to_string());
    }

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

    if attachments.len() > ATTACHMENTS_MAX_COUNT {
        return Ok(format!(
            "添付ファイル数が多すぎます({}ファイル)。{}ファイル以下にしてください。",
            attachments.len(),
            ATTACHMENTS_MAX_COUNT,
        ));
    }

    // we cannot use iter().mao() because download method is async function.
    let mut downloadad_attachments = vec![];

    for attachment in attachments {
        downloadad_attachments.push(MessageAliasAttachment {
            name: attachment.name().to_string(),
            data: attachment.download().await?,
        });
    }

    let entry = MessageAlias {
        key: key.into(),
        message: msg.into(),
        created_at: Utc::now(),
        attachments: downloadad_attachments,
    };

    db.write()
        .await
        .save(entry)
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
