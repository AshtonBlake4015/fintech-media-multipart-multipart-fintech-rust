use fintech_media_multipart::infrai_storage::{CompletedPart, InfraiStorage, StorageError};
use fintech_media_multipart::risk_policy::{decide_upload, AuditNotification, PaymentMedia, UploadDecision};
use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncReadExt, BufReader};

const BUCKET: &str = "fintech-payment-media";

#[derive(Debug, Error)]
enum UploadError {
    #[error("usage: payment_media_uploader <payment-id> <risk-score> <media-path>")]
    Usage,
    #[error("invalid risk score: {0}")]
    RiskScore(#[from] std::num::ParseIntError),
    #[error("media I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("upload held for manual review: {0}")]
    ManualReview(&'static str),
    #[error("media path has no file name")]
    MissingFileName,
}

#[tokio::main]
async fn main() -> Result<(), UploadError> {
    let mut args = std::env::args().skip(1);
    let payment_id = args.next().ok_or(UploadError::Usage)?;
    let risk_score = args.next().ok_or(UploadError::Usage)?.parse::<u8>()?;
    let media_path = args.next().ok_or(UploadError::Usage)?;
    if args.next().is_some() {
        return Err(UploadError::Usage);
    }

    let path = Path::new(&media_path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UploadError::MissingFileName)?;
    let bytes = tokio::fs::metadata(path).await?.len();
    let media = PaymentMedia {
        object_key: format!("evidence/{payment_id}/{file_name}"),
        content_type: content_type(path).into(),
        payment_id,
        bytes,
        risk_score,
    };
    let decision = decide_upload(&media);
    println!(
        "{}",
        serde_json::to_string(&AuditNotification {
            payment_id: &media.payment_id,
            object_key: &media.object_key,
            bytes: media.bytes,
            risk_score: media.risk_score,
            decision: &decision,
        })
        .expect("audit notification is serializable")
    );

    let UploadDecision::Approved { part_size } = decision else {
        let UploadDecision::ManualReview { reason } = decision else {
            unreachable!()
        };
        return Err(UploadError::ManualReview(reason));
    };

    let storage = InfraiStorage::from_env()?;
    storage.create_bucket(BUCKET).await?;
    let request_id = format!("payment-media:{}:{}", media.payment_id, media.bytes);
    let upload = storage
        .create_multipart(
            BUCKET,
            &media.object_key,
            &media.content_type,
            &format!("{request_id}:create"),
        )
        .await?;

    let file = tokio::fs::File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut completed = Vec::new();
    let mut part_number = 1_u32;
    loop {
        let mut chunk = vec![0_u8; part_size as usize];
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        chunk.truncate(read);
        let signed = storage
            .presign_part(&upload.upload_id, part_number)
            .await?;
        let etag = storage.put_signed_part(&signed.url, chunk).await?;
        completed.push(CompletedPart { part_number, etag });
        part_number += 1;
    }

    let stored = storage
        .complete_multipart(
            &upload.upload_id,
            &completed,
            &format!("{request_id}:complete"),
        )
        .await?;
    println!(
        "{}",
        serde_json::json!({
            "payment_id": media.payment_id,
            "object_key": stored.key,
            "parts": completed.len(),
            "state": "stored"
        })
    );
    Ok(())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("mp4") => "video/mp4",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}

