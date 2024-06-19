use {
    crate::bot::{
        alias::{
            model::{MessageAlias, MessageAliasAttachment},
            MessageAliasBot, MessageAliasDatabase,
        },
        Attachment, Context,
    },
    anyhow::{anyhow, Context as _, Result},
    chrono::Utc,
    image::io::Reader as ImageReader,
    libwebp_sys::{WebPConfig, WebPPicture, WebPPreset},
    static_assertions::const_assert,
    std::{io::Cursor, mem::MaybeUninit},
};

const KEY_LENGTH_LIMIT: usize = 100;
const MSG_LENGTH_LIMIT: usize = 2000;
const ATTACHMENTS_MAX_COUNT: usize = 1;
const MAX_FILE_SIZE: usize = 512 * 1024;

const MAX_COMPRESSABLE_FILE_SIZE: usize = 1024 * 1024 * 3;
const COMPRESSABLE_FILE_EXTENSIONS: &[&str] = &[".jpg", ".jpeg", ".png"];
const COMPRESS_TARGET_SIZE: usize = 1024 * 256;

const_assert!(MAX_COMPRESSABLE_FILE_SIZE > MAX_FILE_SIZE);
const_assert!(COMPRESS_TARGET_SIZE <= MAX_FILE_SIZE);

impl<D: MessageAliasDatabase> MessageAliasBot<D> {
    pub(super) async fn status(&self) -> Result<String> {
        let len = self.db.len().await?;
        Ok(format!("```\n現在登録されているエイリアス数: {len}\n```"))
    }

    pub(super) async fn usage_ranking(&self) -> Result<String> {
        const SHOW_COUNT: usize = 20;
        let ranking = self.db.usage_count_top_n(SHOW_COUNT).await?;

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

    pub(super) async fn make(
        &self,
        ctx: &dyn Context,
        key: &str,
        msg: Option<&str>,
        attachments: &[&dyn Attachment],
        force: bool,
    ) -> Result<()> {
        let key = key.trim();
        let msg = msg.unwrap_or("").trim();
        let mut error_msgs = vec![];

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

        let mut straight_download = vec![];
        let mut compress_download = vec![];

        for attachment in attachments {
            let ext = COMPRESSABLE_FILE_EXTENSIONS
                .iter()
                .any(|x| attachment.name().ends_with(x));

            let max_size_exceed = attachment.size() > MAX_FILE_SIZE;
            let compress_max_size_exceed = attachment.size() > MAX_COMPRESSABLE_FILE_SIZE;

            match (ext, max_size_exceed, compress_max_size_exceed) {
                (_, _, true) | (false, true, false) => {
                    error_msgs.push(format!(
                        "添付ファイル(\"{}\")のサイズが大きすぎます({:.2}MiB)。{}MiB以下にしてください。",
                        attachment.name(),
                        mib(attachment.size()),
                        mib(MAX_FILE_SIZE),
                    ));
                }
                (true, true, false) => compress_download.push(attachment),
                (_, false, false) => straight_download.push(attachment),
            }
        }

        let key_len = key.chars().count();
        let msg_len = msg.chars().count();

        if key_len > KEY_LENGTH_LIMIT {
            error_msgs.push(format!(
                "長すぎるキー({key_len}文字)です。{KEY_LENGTH_LIMIT}文字以下にしてください。",
            ));
        }

        if msg_len > MSG_LENGTH_LIMIT {
            error_msgs.push(format!(
                "長すぎるメッセージ({msg_len}文字)です。{MSG_LENGTH_LIMIT}文字以下にしてください。",
            ));
        }

        if !error_msgs.is_empty() {
            ctx.send_text_message(&error_msgs.join("\n")).await?;
            return Ok(());
        }

        let mut force_applied = false;

        if let Some(alias) = self.db.get(key).await? {
            if !force {
                ctx.send_text_message("すでにそのキーにはエイリアスが登録されています。上書きしたい場合は先に削除するか、`-f` オプションを使用することで強制的に上書き登録できます。\n現在登録されているエイリアスを続けて送信します。").await?;
                self.send_alias(ctx, &alias).await?;
                return Ok(());
            }

            self.db.delete(key).await?;
            force_applied = true;
        }

        let mut downloaded_attachments = vec![];
        let mut compress_messages = vec![];

        for attachment in compress_download {
            let name = attachment.name();
            let data = attachment.download().await?;
            match compress(&data) {
                Ok(compressed) => {
                    if compressed.len() > MAX_FILE_SIZE {
                        error_msgs.push(format!(
                            "添付ファイル({name})の圧縮を試みましたが、十分に小さく出来ませんでした({:.02}MiB -> {:.02}MiB)",
                            mib(data.len()),
                            mib(compressed.len()),
                        ));
                        continue;
                    }

                    compress_messages.push(format!(
                        "添付ファイル({name})は圧縮されました({:.02}MiB -> {:.02}MiB)",
                        mib(data.len()),
                        mib(compressed.len()),
                    ));
                    downloaded_attachments.push(MessageAliasAttachment {
                        name: attachment.name().to_string(),
                        data: compressed,
                    });
                }

                Err(CompressError::Guess(e)) => {
                    tracing::info!("{name} compress failed: guess: {e}");
                    error_msgs.push(format!(
                        "添付ファイル({name})の圧縮を試みましたが、画像フォーマットの推測に失敗しました",
                    ));
                }
                Err(CompressError::Decode(e)) => {
                    tracing::info!("{name} compress failed: decode: {e}");
                    error_msgs.push(format!(
                        "添付ファイル({name})の圧縮を試みましたが、画像のデコードに失敗しました",
                    ));
                }
                Err(CompressError::Compress(e)) => {
                    tracing::info!("{name} compress failed: compress: {e}");
                    error_msgs.push(format!(
                        "添付ファイル({name})の圧縮を試みましたが、画像のエンコードに失敗しました",
                    ));
                }
            }
        }

        if !error_msgs.is_empty() {
            ctx.send_text_message(&error_msgs.join("\n")).await?;
            return Ok(());
        }

        for attachment in straight_download {
            downloaded_attachments.push(MessageAliasAttachment {
                name: attachment.name().to_string(),
                data: attachment.download().await?,
            });
        }

        let entry = MessageAlias {
            key: key.into(),
            message: msg.into(),
            created_at: Utc::now(),
            attachments: downloaded_attachments,
            usage_count: 0,
        };

        self.db
            .save(entry)
            .await
            .context("failed to save new alias")?;

        let mut message = compress_messages.join("\n");

        message += if force_applied {
            "\n既存のエイリアスを削除して作成しました"
        } else {
            "\n作成しました"
        };

        ctx.send_text_message(message.trim()).await?;

        Ok(())
    }

    pub(super) async fn delete(&self, key: &str) -> Result<String> {
        let deleted = self
            .db
            .delete(key)
            .await
            .context("failed to delete alias")?;

        if deleted {
            Ok("削除しました".into())
        } else {
            Ok("そのようなキーを持つエイリアスはありません".into())
        }
    }
}

#[derive(Debug)]
enum CompressError {
    Guess(std::io::Error),
    Decode(image::ImageError),
    Compress(anyhow::Error),
}

#[cfg(test)]
#[test]
fn compress_test() {
    use image::{ImageFormat, RgbImage};

    let mut img = RgbImage::new(512, 512);
    for pixel in img.pixels_mut() {
        pixel.0 = [255, 0, 0];
    }

    let mut png = vec![];
    img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();

    compress(&png).unwrap();
}

fn compress(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    use CompressError::*;

    let src_image = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(Guess)?
        .decode()
        .map_err(Decode)?
        .into_rgba8();

    let mut config = WebPConfig::new_with_preset(WebPPreset::WEBP_PRESET_DEFAULT, 80.0)
        .map_err(|()| Compress(anyhow!("failed to create WebPConfig")))?;

    config.target_size = COMPRESS_TARGET_SIZE as i32;

    if unsafe { libwebp_sys::WebPValidateConfig(&config) } == 0 {
        return Err(Compress(anyhow!(
            "WebPValidateConfig failed. config was:\n{config:#?}"
        )));
    }

    let mut webp_picture =
        WebPPicture::new().map_err(|()| Compress(anyhow!("failed to create WebPPicture")))?;

    webp_picture.width = src_image.width() as i32;
    webp_picture.height = src_image.height() as i32;

    if unsafe { libwebp_sys::WebPPictureAlloc(&mut webp_picture) } == 0 {
        return Err(Compress(anyhow!("failed to allocate WebPPicture")));
    }

    let width = src_image.width() as i32;

    if unsafe {
        libwebp_sys::WebPPictureImportRGBA(&mut webp_picture, src_image.as_ptr(), width * 4)
    } == 0
    {
        return Err(Compress(anyhow!(
            "failed to copy source image to libwebp_sys"
        )));
    }

    let mut writer = unsafe {
        let mut uninit = MaybeUninit::uninit();
        libwebp_sys::WebPMemoryWriterInit(uninit.as_mut_ptr());
        uninit.assume_init()
    };

    webp_picture.writer = Some(libwebp_sys::WebPMemoryWrite);
    webp_picture.custom_ptr = &mut writer as *mut _ as _;

    let res = unsafe { libwebp_sys::WebPEncode(&config, &mut webp_picture) };

    // always free the input memory
    unsafe { libwebp_sys::WebPPictureFree(&mut webp_picture) };

    if res == 0 {
        return Err(Compress(anyhow!("WebPEncode failed")));
    }

    let output = unsafe { std::slice::from_raw_parts(writer.mem, writer.size).to_vec() };

    unsafe { libwebp_sys::WebPMemoryWriterClear(&mut writer) };

    Ok(output)
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
