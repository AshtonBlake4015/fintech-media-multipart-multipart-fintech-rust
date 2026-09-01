use serde::Serialize;

#[derive(Debug, Clone)]
pub struct PaymentMedia {
    pub payment_id: String,
    pub object_key: String,
    pub content_type: String,
    pub bytes: u64,
    pub risk_score: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum UploadDecision {
    Approved { part_size: u64 },
    ManualReview { reason: &'static str },
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditNotification<'a> {
    pub payment_id: &'a str,
    pub object_key: &'a str,
    pub bytes: u64,
    pub risk_score: u8,
    pub decision: &'a UploadDecision,
}

pub fn decide_upload(media: &PaymentMedia) -> UploadDecision {
    if media.risk_score >= 80 {
        return UploadDecision::ManualReview {
            reason: "risk_score_requires_review",
        };
    }

    UploadDecision::Approved {
        part_size: 8 * 1024 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_risk_payment_media_is_held_before_storage_changes() {
        let media = PaymentMedia {
            payment_id: "pay_2048".into(),
            object_key: "evidence/pay_2048/receipt.mp4".into(),
            content_type: "video/mp4".into(),
            bytes: 32 * 1024 * 1024,
            risk_score: 91,
        };

        assert_eq!(
            decide_upload(&media),
            UploadDecision::ManualReview {
                reason: "risk_score_requires_review"
            }
        );
    }
}

