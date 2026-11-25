//! The calendar rule of a repeating task (schema 144, designs/agent-tasks). The
//! fields follow the iCalendar RRULE parts people ask for in chat: FREQ (`unit`),
//! INTERVAL (`every`), BYDAY (`days`), BYMONTHDAY (`day_of_month`) and the time of
//! day. The next run is worked out in PostgreSQL (`relay_task_next_due`), which
//! owns the time-zone and daylight-saving arithmetic.
use super::super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rule {
    /// `minute` (a fixed period from the first run), `hour`, `day`, `week` or `month`.
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<i64>,
    /// Local time `HH:MM` for daily, weekly and monthly runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// ISO weekdays, 1 = Monday, for weekly runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<i64>>,
    /// 1 to 31, or -1 for the last day of the month.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_month: Option<i64>,
    /// Local `HH:MM` window for hourly runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Only on the workspace's working days; holidays follow its rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_days: Option<bool>,
}

/// Minutes after midnight of a local `HH:MM`, or an error naming `what`.
fn clock(text: &str, what: &str) -> Result<u32> {
    let parsed = text.split_once(':').and_then(|(hs, ms)| {
        let shape = (1..=2).contains(&hs.len()) && ms.len() == 2;
        let (h, m) = (hs.parse::<u32>().ok()?, ms.parse::<u32>().ok()?);
        (shape && h < 24 && m < 60).then_some(h * 60 + m)
    });
    parsed.with_context(|| format!("{what} is a 24-hour time like 09:00"))
}

fn shown(text: &str) -> String {
    let minutes = clock(text, "time").unwrap_or(9 * 60);
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn ordinal(day: i64) -> String {
    let suffix = match (day % 10, day % 100) {
        (1, n) if n != 11 => "st",
        (2, n) if n != 12 => "nd",
        (3, n) if n != 13 => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl Rule {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.unit == "minute" {
            ensure!(
                self.every.is_some_and(|n| (15..=1440).contains(&n)),
                "A minute rule runs every 15 to 1440 minutes"
            );
            ensure!(
                self.at.is_none()
                    && self.from.is_none()
                    && self.to.is_none()
                    && self.days.is_none()
                    && self.day_of_month.is_none(),
                "A minute rule only has every"
            );
            return Ok(());
        }
        let max = match self.unit.as_str() {
            "hour" => 24,
            "day" => 365,
            "week" => 52,
            "month" => 12,
            _ => anyhow::bail!("unit is minute, hour, day, week or month"),
        };
        ensure!(
            self.every.is_none_or(|n| (1..=max).contains(&n)),
            "every is 1 to {max} for a rule in {}s",
            self.unit
        );
        let hourly = self.unit == "hour";
        ensure!(
            !hourly || self.at.is_none(),
            "An hourly rule uses from and to, not at"
        );
        ensure!(
            hourly || (self.from.is_none() && self.to.is_none()),
            "from and to are only for hourly rules"
        );
        if let Some(at) = &self.at {
            clock(at, "at")?;
        }
        if let (Some(from), Some(to)) = (&self.from, &self.to) {
            ensure!(
                clock(from, "from")? < clock(to, "to")?,
                "from is earlier than to"
            );
        } else {
            for (time, what) in [(&self.from, "from"), (&self.to, "to")] {
                if let Some(time) = time {
                    clock(time, what)?;
                }
            }
        }
        if let Some(days) = &self.days {
            ensure!(self.unit == "week", "days are only for weekly rules");
            ensure!(
                !days.is_empty() && days.iter().all(|d| (1..=7).contains(d)),
                "days are weekdays 1 (Monday) to 7 (Sunday)"
            );
            let mut unique = days.clone();
            unique.sort_unstable();
            unique.dedup();
            ensure!(unique.len() == days.len(), "Name each day once");
        }
        if let Some(day) = self.day_of_month {
            ensure!(
                self.unit == "month",
                "day_of_month is only for monthly rules"
            );
            ensure!(
                day == -1 || (1..=31).contains(&day),
                "day_of_month is 1 to 31, or -1 for the last day"
            );
        }
        Ok(())
    }

    /// About how long one period lasts, for the missed-run window.
    pub(crate) fn period_ms(&self) -> i64 {
        let unit = match self.unit.as_str() {
            "minute" => 60_000,
            "hour" => 3_600_000,
            "day" => 86_400_000,
            "week" => 7 * 86_400_000,
            _ => 30 * 86_400_000,
        };
        unit * self.every.unwrap_or(1).max(1)
    }

    /// The rule as one sentence: "Every weekday at 9:00, on working days".
    pub(crate) fn sentence(&self) -> String {
        let every = self.every.unwrap_or(1).max(1);
        let at = format!(" at {}", shown(self.at.as_deref().unwrap_or("09:00")));
        let base = match self.unit.as_str() {
            // Tasks converted in schema 147 may have periods of whole hours past a day.
            "minute" if every % 60 == 0 && every >= 120 => format!("Every {} hours", every / 60),
            "minute" => format!("Every {every} minutes"),
            "hour" => {
                let head = if every == 1 {
                    "Every hour".to_string()
                } else {
                    format!("Every {every} hours")
                };
                match (&self.from, &self.to) {
                    (Some(from), Some(to)) => {
                        format!("{head} from {} to {}", shown(from), shown(to))
                    }
                    (Some(from), None) => format!("{head} from {}", shown(from)),
                    (None, Some(to)) => format!("{head} until {}", shown(to)),
                    (None, None) => head,
                }
            }
            "day" if every == 1 => format!("Every day{at}"),
            "day" => format!("Every {every} days{at}"),
            "week" => {
                let mut days = self.days.clone().unwrap_or_default();
                days.sort_unstable();
                let names = if days == [1, 2, 3, 4, 5] {
                    "weekday".to_string()
                } else if days.is_empty() {
                    "week".to_string()
                } else {
                    list(
                        &days
                            .iter()
                            .map(|d| WEEKDAYS[(*d - 1) as usize].to_string())
                            .collect::<Vec<_>>(),
                    )
                };
                if every == 1 {
                    format!("Every {names}{at}")
                } else {
                    format!("Every {every} weeks on {names}{at}")
                }
            }
            _ => {
                let day = match self.day_of_month {
                    Some(-1) => " on the last day".to_string(),
                    Some(day) => format!(" on the {}", ordinal(day)),
                    None => String::new(),
                };
                if every == 1 {
                    format!("Every month{day}{at}")
                } else {
                    format!("Every {every} months{day}{at}")
                }
            }
        };
        if self.working_days == Some(true) {
            format!("{base}, on working days")
        } else {
            base
        }
    }
}

/// The next run of `rule` after `after_ms`, in local time of `zone`, counting from
/// `anchor_ms`, with the days off of workspace `w`; none within 400 days.
pub(crate) async fn next_due(
    db: &impl ConnectionTrait,
    w: &str,
    rule: &Rule,
    zone: &str,
    anchor_ms: i64,
    after_ms: i64,
) -> Result<Option<i64>> {
    Ok(query(
        db,
        "SELECT relay_task_next_due($1::jsonb,$2,$3,$4,$5) AS due",
        vec![
            serde_json::to_value(rule)?.into(),
            zone.into(),
            anchor_ms.into(),
            after_ms.into(),
            w.into(),
        ],
    )
    .await?
    .context("Next run unavailable")?
    .try_get("", "due")?)
}

#[cfg(test)]
mod tests {
    use super::Rule;
    use serde_json::json;

    fn rule(value: serde_json::Value) -> Rule {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn rules_read_as_one_sentence() {
        let cases = [
            (json!({"unit":"day","at":"10:00"}), "Every day at 10:00"),
            (
                json!({"unit":"day","every":2,"at":"10:00"}),
                "Every 2 days at 10:00",
            ),
            (
                json!({"unit":"week","days":[1,2,3,4,5],"at":"09:00"}),
                "Every weekday at 9:00",
            ),
            (
                json!({"unit":"week","days":[4,1],"at":"17:30"}),
                "Every Monday and Thursday at 17:30",
            ),
            (
                json!({"unit":"week","every":2,"days":[5]}),
                "Every 2 weeks on Friday at 9:00",
            ),
            (
                json!({"unit":"month","day_of_month":-1,"at":"18:00","working_days":true}),
                "Every month on the last day at 18:00, on working days",
            ),
            (
                json!({"unit":"month","every":3,"day_of_month":22}),
                "Every 3 months on the 22nd at 9:00",
            ),
            (
                json!({"unit":"hour","from":"09:00","to":"19:00"}),
                "Every hour from 9:00 to 19:00",
            ),
            (json!({"unit":"minute","every":90}), "Every 90 minutes"),
        ];
        for (value, sentence) in cases {
            let rule = rule(value);
            rule.validate().unwrap();
            assert_eq!(rule.sentence(), sentence);
        }
        // A task converted in schema 147 may keep a period past a day; it is read
        // and shown, though an agent can't give one.
        let long = rule(json!({"unit":"minute","every":2160}));
        assert!(long.validate().is_err());
        assert_eq!(long.sentence(), "Every 36 hours");
        assert_eq!(long.period_ms(), 2160 * 60_000);
    }

    #[test]
    fn rules_outside_their_bounds_are_refused() {
        for value in [
            json!({"unit":"second"}),
            json!({"unit":"minute"}),
            json!({"unit":"minute","every":5}),
            json!({"unit":"minute","every":30,"at":"09:00"}),
            json!({"unit":"day","every":0}),
            json!({"unit":"month","every":13}),
            json!({"unit":"day","at":"25:00"}),
            json!({"unit":"day","at":"9"}),
            json!({"unit":"hour","at":"09:00"}),
            json!({"unit":"day","from":"09:00"}),
            json!({"unit":"hour","from":"19:00","to":"09:00"}),
            json!({"unit":"week","days":[]}),
            json!({"unit":"week","days":[8]}),
            json!({"unit":"week","days":[1,1]}),
            json!({"unit":"day","days":[1]}),
            json!({"unit":"month","day_of_month":0}),
            json!({"unit":"week","day_of_month":3}),
        ] {
            assert!(rule(value.clone()).validate().is_err(), "{value}");
        }
        assert!(
            serde_json::from_value::<Rule>(json!({"unit":"day","cron":"* * *"})).is_err(),
            "unknown fields are refused"
        );
    }
}
