//! What SES puts on Cuckoo's queue: a mail it received for the domain (stored
//! in S3, with its authentication verdicts) or what became of a reply Cuckoo
//! sent (configuration set events). The queue accepts messages only from
//! Cuckoo's SNS topics, which only SES publishes to, so the verdicts can be
//! trusted. Both the SNS envelope and raw delivery are accepted.
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, PartialEq)]
pub(crate) enum Notice {
    Received(Received),
    /// What became of a reply, for the attempt named in its tags.
    Outcome {
        workspace: String,
        key: String,
        delivered: bool,
        reason: Option<String>,
    },
    /// Anything else (a test notification, a send or delay event): nothing to do.
    Other,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Received {
    /// SES's own ID for the mail; unique, so it deduplicates redeliveries.
    pub ses_id: String,
    /// Envelope recipients on Cuckoo's domain: who the mail was really for.
    pub recipients: Vec<String>,
    /// DMARC passed: the `From` domain authenticated the mail (SPF or DKIM, aligned).
    pub dmarc: bool,
    /// SES found spam or a virus.
    pub unsafe_content: bool,
    pub bucket: String,
    pub key: String,
}

#[derive(Deserialize)]
struct Verdict {
    status: String,
}

pub(crate) fn parse(body: &str) -> Result<Notice> {
    let outer: Value = serde_json::from_str(body).context("Unreadable queue message")?;
    // SNS wraps the notification as a JSON string unless raw delivery is on.
    let notice: Value = match outer["Type"].as_str() {
        Some("Notification") => serde_json::from_str(
            outer["Message"]
                .as_str()
                .context("SNS message without content")?,
        )
        .context("Unreadable SES notification")?,
        Some(_) => return Ok(Notice::Other),
        None => outer,
    };
    if notice["notificationType"] == "Received" {
        return received(&notice).map(Notice::Received);
    }
    let outcome = match notice["eventType"].as_str() {
        Some("Delivery") => Some((true, None)),
        Some("Bounce") if notice["bounce"]["bounceType"] == "Permanent" => Some((
            false,
            Some("The address does not exist or refused mail".to_owned()),
        )),
        Some("Complaint") => Some((
            false,
            Some("The person marked the reply as spam".to_owned()),
        )),
        Some("Reject") => Some((false, Some("SES refused to send the reply".to_owned()))),
        _ => None,
    };
    let Some((delivered, reason)) = outcome else {
        return Ok(Notice::Other);
    };
    let tag = |name: &str| notice["mail"]["tags"][name][0].as_str().map(str::to_owned);
    match (tag("workspace"), tag("attempt")) {
        (Some(workspace), Some(key)) => Ok(Notice::Outcome {
            workspace,
            key,
            delivered,
            reason,
        }),
        _ => Ok(Notice::Other),
    }
}

fn received(notice: &Value) -> Result<Received> {
    let receipt = &notice["receipt"];
    let pass = |name: &str| {
        serde_json::from_value::<Verdict>(receipt[name].clone())
            .map(|v| v.status == "PASS")
            .unwrap_or(false)
    };
    let fail = |name: &str| {
        serde_json::from_value::<Verdict>(receipt[name].clone())
            .map(|v| v.status == "FAIL")
            .unwrap_or(false)
    };
    let action = &receipt["action"];
    anyhow::ensure!(
        action["type"] == "S3",
        "The receipt rule must store mail in S3"
    );
    Ok(Received {
        ses_id: notice["mail"]["messageId"]
            .as_str()
            .context("Mail without an SES ID")?
            .to_owned(),
        recipients: receipt["recipients"]
            .as_array()
            .context("Mail without recipients")?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_lowercase)
            .collect(),
        dmarc: pass("dmarcVerdict"),
        unsafe_content: fail("spamVerdict") || fail("virusVerdict"),
        bucket: action["bucketName"]
            .as_str()
            .context("Mail without a bucket")?
            .to_owned(),
        key: action["objectKey"]
            .as_str()
            .context("Mail without an object key")?
            .to_owned(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn received_body(ses_id: &str, to: &str, dmarc: &str, key: &str) -> String {
        let notice = json!({"notificationType":"Received",
            "mail":{"messageId":ses_id,"destination":[to],"commonHeaders":{"from":["someone"]}},
            "receipt":{"recipients":[to],"spamVerdict":{"status":"PASS"},"virusVerdict":{"status":"PASS"},
                "spfVerdict":{"status":"PASS"},"dkimVerdict":{"status":"PASS"},"dmarcVerdict":{"status":dmarc},
                "action":{"type":"S3","bucketName":"ayati-mail","objectKey":key}}});
        json!({"Type":"Notification","Message":notice.to_string()}).to_string()
    }

    #[test]
    fn received_mail_carries_its_verdicts_and_where_it_is_stored() {
        let Notice::Received(mail) = parse(&received_body(
            "ses-1",
            "Nova.Acme@mail.cuckoo.onl",
            "PASS",
            "in/ses-1",
        ))
        .unwrap() else {
            panic!("a received mail")
        };
        assert_eq!(mail.recipients, ["nova.acme@mail.cuckoo.onl"]);
        assert!(mail.dmarc && !mail.unsafe_content);
        assert_eq!(
            (mail.bucket.as_str(), mail.key.as_str()),
            ("ayati-mail", "in/ses-1")
        );
        for verdict in ["FAIL", "GRAY", "PROCESSING_FAILED"] {
            let Notice::Received(mail) = parse(&received_body(
                "ses-2",
                "nova@mail.cuckoo.onl",
                verdict,
                "k",
            ))
            .unwrap() else {
                panic!("a received mail")
            };
            assert!(!mail.dmarc, "{verdict}");
        }
    }

    #[test]
    fn delivery_events_name_their_attempt_and_others_are_ignored() {
        let event = |kind: &str, extra: Value| {
            let mut notice = json!({"eventType":kind,"mail":{"messageId":"m","tags":{"workspace":["w1"],"attempt":["k1"]}}});
            notice
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            notice.to_string()
        };
        assert_eq!(
            parse(&event("Delivery", json!({}))).unwrap(),
            Notice::Outcome {
                workspace: "w1".into(),
                key: "k1".into(),
                delivered: true,
                reason: None
            }
        );
        assert!(matches!(
            parse(&event(
                "Bounce",
                json!({"bounce":{"bounceType":"Permanent"}})
            ))
            .unwrap(),
            Notice::Outcome {
                delivered: false,
                ..
            }
        ));
        assert_eq!(
            parse(&event(
                "Bounce",
                json!({"bounce":{"bounceType":"Transient"}})
            ))
            .unwrap(),
            Notice::Other
        );
        assert_eq!(parse(&event("Send", json!({}))).unwrap(), Notice::Other);
        assert_eq!(
            parse(r#"{"Type":"SubscriptionConfirmation"}"#).unwrap(),
            Notice::Other
        );
        assert!(parse("not json").is_err());
    }
}
