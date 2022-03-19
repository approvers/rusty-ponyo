use {
    crate::bot::{
        alias::{
            model::{MessageAlias, MessageAliasAttachment},
            MessageAliasDatabase,
        },
        Attachment,
    },
    anyhow::{Context as _, Result},
    chrono::Utc,
};

pub(super) async fn status(db: &impl MessageAliasDatabase) -> Result<String> {
    let len = db.len().await?;
    Ok(format!("```\n現在登録されているエイリアス数: {}\n```", len))
}

pub(super) async fn usage_ranking(db: &impl MessageAliasDatabase) -> Result<String> {
    const SHOW_COUNT: usize = 20;
    let ranking = db.usage_count_top_n(SHOW_COUNT).await?;

    let mut result = vec!["```".into()];
    for (i, r) in ranking.into_iter().enumerate() {
        result.push(format!(
            "#{:02} 使用回数: {:3} \"{}\"",
            i + 1,
            r.usage_count,
            r.key
        ));
    }

    result.push("```".into());

    Ok(result.join("\n"))
}

const KEY_LENGTH_LIMIT: usize = 100;
const MSG_LENGTH_LIMIT: usize = 500;
const ATTACHMENTS_MAX_COUNT: usize = 1;
const MAX_FILE_SIZE: usize = 1024 * 512;

pub(super) async fn make(
    db: &impl MessageAliasDatabase,
    key: &str,
    msg: Option<&str>,
    attachments: &[&dyn Attachment],
) -> Result<String> {
    let key = key.trim();
    let msg = msg.unwrap_or("").trim();

    if db.get(key).await?.is_some() {
        return Ok("すでにそのキーにはエイリアスが登録されています。上書きしたい場合は先に削除してください。".to_string());
    }

    let mut error_msgs = vec![];

    let key_len = key.chars().count();
    let msg_len = msg.chars().count();

    if key.is_empty() {
        error_msgs.push("キーが空白です。".to_string());
    }

    if attachments.is_empty() && msg.is_empty() {
        error_msgs.push("メッセージもしくは添付ファイルのどちらかは必ず必要です。".to_string());
    }

    if attachments.len() > ATTACHMENTS_MAX_COUNT {
        error_msgs.push(format!(
            "添付ファイル数が多すぎます({}ファイル)。{}ファイル以下にしてください。",
            attachments.len(),
            ATTACHMENTS_MAX_COUNT,
        ));
    }

    for attachment in attachments {
        if attachment.size() > MAX_FILE_SIZE {
            error_msgs.push(format!(
                "添付ファイル(\"{}\")のサイズが大きすぎます({:.2}KB)。{}KB以下にしてください。",
                attachment.name(),
                attachment.size() as f64 / 1024.0,
                MAX_FILE_SIZE / 1024,
            ));
        }
    }

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

    // we cannot use iter().map() because download method is async function.
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
        usage_count: 0,
    };

    db.save(entry).await.context("failed to save new alias")?;

    Ok("作成しました".into())
}

pub(super) async fn delete(db: &impl MessageAliasDatabase, key: &str) -> Result<String> {
    let deleted = db.delete(key).await.context("failed to delete alias")?;

    if deleted {
        Ok("削除しました".into())
    } else {
        Ok("そのようなキーを持つエイリアスはありません".into())
    }
}
