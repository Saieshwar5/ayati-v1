//! Keeping a note on a shelf once, and what a note may hold: never a secret,
//! and a date it holds until that is real and ahead.
use super::*;

/// Whether text looks like a password, key or card number: never kept.
pub(crate) fn secret(text: &str) -> bool {
    let lower = text.to_lowercase();
    let words = [
        "password",
        "passwd",
        "api key",
        "apikey",
        "secret key",
        "private key",
        "otp",
        "pin code",
        "cvv",
    ];
    let card = text
        .split(|c: char| !(c.is_ascii_digit() || c == ' ' || c == '-'))
        .any(|run| (13..=19).contains(&run.chars().filter(char::is_ascii_digit).count()));
    let token = text.split_whitespace().any(|w| {
        w.len() >= 24
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && w.chars().any(|c| c.is_ascii_digit())
            && w.chars().any(|c| c.is_ascii_alphabetic())
    });
    words.iter().any(|w| lower.contains(w)) || card || token
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// The start of `date` (YYYY-MM-DD, UTC) in milliseconds, when it is a real date.
pub(crate) fn day_start_ms(date: &str) -> Option<i64> {
    let date = date.trim();
    let parts: Vec<i64> = date
        .split('-')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day] = parts[..] else {
        return None;
    };
    let length = match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    ((1..=length).contains(&day) && date.len() == 10).then(|| days(year, month, day) * 86_400_000)
}

/// The end of `date` in milliseconds, when it is ahead of `now_ms` and within two years.
pub(crate) fn until_ms(date: &str, now_ms: i64) -> Option<i64> {
    let end = day_start_ms(date)? + 86_400_000 - 1000;
    (end > now_ms && end < now_ms + 2 * 366 * 86_400_000).then_some(end)
}

/// One note to keep on a shelf.
pub(crate) struct NewNote<'a> {
    pub id: &'a str,
    pub shelf: &'a Shelf,
    pub kind: &'a str,
    pub topic: &'a str,
    pub text: &'a str,
    pub source: &'a str,
    pub author: Option<&'a str>,
    pub job: Option<&'a str>,
    pub learned_in: Option<&'a str>,
    pub strength: i32,
    pub waiting: Option<&'a str>,
    pub until_ms: Option<i64>,
    pub replaces: Option<&'a str>,
}

/// Keeps `note` once per shelf. Returns `kept`, `known` (the same words are
/// already there, which makes them stronger), `waiting_known` or `full`.
pub(crate) async fn keep(
    tx: &DatabaseTransaction,
    w: &str,
    note: NewNote<'_>,
) -> Result<&'static str> {
    let shelf = note.shelf.key();
    query(
        tx,
        "SELECT pg_advisory_xact_lock(hashtext($1||':memory:'||$2))::text AS locked",
        vec![w.into(), shelf.clone().into()],
    )
    .await?;
    let same = query(
        tx,
        "SELECT id,state,author FROM relay_memories WHERE workspace=$1 AND shelf=$2 AND state IN ('active','waiting')
           AND btrim(regexp_replace(lower(text),'[^[:alnum:]]+',' ','g'))=btrim(regexp_replace(lower($3),'[^[:alnum:]]+',' ','g')) LIMIT 1",
        vec![w.into(), shelf.clone().into(), note.text.into()],
    )
    .await?;
    if let Some(found) = same {
        if found.try_get::<String>("", "state")? == "waiting" {
            return Ok("waiting_known");
        }
        // A second agent learning the same thing makes it much stronger.
        let other = found.try_get::<Option<String>>("", "author")?.as_deref() != note.author;
        execute(
            tx,
            &format!("UPDATE relay_memories SET strength=least(100,strength+$3),used_ms={NOW_MS},used=used+1 WHERE workspace=$1 AND id=$2"),
            vec![w.into(), found.try_get::<String>("", "id")?.into(), if other { 15 } else { 5 }.into()],
        )
        .await?;
        return Ok("known");
    }
    let count: i64 = query(
        tx,
        "SELECT count(*) AS n FROM relay_memories WHERE workspace=$1 AND shelf=$2 AND state IN ('active','waiting')",
        vec![w.into(), shelf.into()],
    )
    .await?
    .context("Missing memory count")?
    .try_get("", "n")?;
    if count >= most(note.shelf.scope) {
        return Ok("full");
    }
    execute(
        tx,
        "INSERT INTO relay_memories(workspace,id,scope,agent,chat,user_id,kind,topic,text,source,author,job,learned_in,state,waiting_reason,strength,until_ms,replaces)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        vec![
            w.into(),
            note.id.into(),
            note.shelf.scope.into(),
            note.shelf.agent.clone().into(),
            note.shelf.chat.clone().into(),
            note.shelf.user.clone().into(),
            note.kind.into(),
            note.topic.into(),
            note.text.into(),
            note.source.into(),
            note.author.map(str::to_owned).into(),
            note.job.map(str::to_owned).into(),
            note.learned_in.map(str::to_owned).into(),
            if note.waiting.is_some() { "waiting" } else { "active" }.into(),
            note.waiting.map(str::to_owned).into(),
            note.strength.into(),
            note.until_ms.into(),
            note.replaces.map(str::to_owned).into(),
        ],
    )
    .await?;
    Ok("kept")
}
