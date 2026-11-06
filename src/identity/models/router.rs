//! Choosing a provider for each model call (MG1). Routes are tried by priority; routes
//! of one priority share traffic by weight. A provider that just failed rests for a
//! short while, so the next calls go to the others; when every route is resting, they
//! are tried anyway rather than failing outright. Health is per controller and kept in
//! memory: it is a hint for ordering, never a reason to refuse.
use super::catalog::{Model, Route};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a provider rests after a failure.
pub const REST: Duration = Duration::from_secs(30);

static RESTING: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// The provider just failed: let it rest.
pub fn failed(provider: &str) {
    if let Ok(mut resting) = RESTING.lock() {
        resting
            .get_or_insert_with(HashMap::new)
            .insert(provider.into(), Instant::now() + REST);
    }
}
/// The provider answered: it is healthy again.
pub fn succeeded(provider: &str) {
    if let Ok(mut resting) = RESTING.lock()
        && let Some(map) = resting.as_mut()
    {
        map.remove(provider);
    }
}
fn resting(provider: &str, now: Instant) -> bool {
    RESTING
        .lock()
        .ok()
        .and_then(|r| r.as_ref().and_then(|m| m.get(provider).copied()))
        .is_some_and(|until| until > now)
}

/// The order to try a model's routes in, for one call. `roll` is a random number,
/// passed in so tests can fix it.
pub fn order(model: &Model, roll: u64) -> Vec<Route> {
    order_at(model, roll, Instant::now())
}
fn order_at(model: &Model, mut roll: u64, now: Instant) -> Vec<Route> {
    let mut priorities: Vec<u8> = model.routes.iter().map(|r| r.priority).collect();
    priorities.sort_unstable();
    priorities.dedup();
    let mut ordered = vec![];
    for priority in priorities {
        let mut group: Vec<Route> = model
            .routes
            .iter()
            .filter(|r| r.priority == priority)
            .cloned()
            .collect();
        // A weighted shuffle: each pick takes a route with chance in proportion to weight.
        while !group.is_empty() {
            let total: u64 = group.iter().map(|r| u64::from(r.weight)).sum();
            let mut point = roll % total;
            roll = roll
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let index = group.iter().position(|r| {
                if point < u64::from(r.weight) {
                    true
                } else {
                    point -= u64::from(r.weight);
                    false
                }
            });
            ordered.push(group.remove(index.unwrap_or(0)));
        }
    }
    // Healthy routes first, keeping the order above; resting ones after, as a last try.
    let (mut healthy, rest): (Vec<Route>, Vec<Route>) = ordered
        .into_iter()
        .partition(|r| !resting(&r.provider, now));
    healthy.extend(rest);
    healthy
}

/// A random number for [`order`], without another dependency.
pub fn roll() -> u64 {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0; 8]))
}

/// Whether a provider's answer means another route should be tried: it couldn't
/// serve the call now (rate limit, overload, outage), not that the request was wrong.
pub fn retryable(status: u16) -> bool {
    status == 429 || status == 408 || (500..=599).contains(&status)
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{Model, Price, Route, Supports};

    fn model(routes: Vec<(&str, u8, u32)>) -> Model {
        Model {
            id: "m".into(),
            name: "M".into(),
            family: String::new(),
            description: String::new(),
            tags: vec![],
            context: 0,
            supports: Supports::default(),
            price: Price::default(),
            status: "available".into(),
            replacement: None,
            routes: routes
                .into_iter()
                .map(|(provider, priority, weight)| Route {
                    provider: provider.into(),
                    model: format!("{provider}-m"),
                    priority,
                    weight,
                    cost: Price::default(),
                })
                .collect(),
        }
    }
    fn providers(routes: &[super::Route]) -> Vec<&str> {
        routes.iter().map(|r| r.provider.as_str()).collect()
    }

    #[test]
    fn priority_comes_first_and_weight_shares_traffic_within_it() {
        let m = model(vec![("backup", 2, 100), ("a", 1, 75), ("b", 1, 25)]);
        let mut first_a = 0;
        for roll in 0..1000u64 {
            let order =
                super::order_at(&m, roll.wrapping_mul(2654435761), std::time::Instant::now());
            assert_eq!(order.len(), 3);
            assert_eq!(
                order[2].provider, "backup",
                "a lower priority is only a fallback"
            );
            if order[0].provider == "a" {
                first_a += 1;
            }
        }
        assert!(
            (650..=850).contains(&first_a),
            "about three in four calls go to a first: {first_a}"
        );
    }

    #[test]
    fn a_failed_provider_rests_then_is_tried_last() {
        let m = model(vec![("rest-a", 1, 100), ("rest-b", 2, 100)]);
        super::failed("rest-a");
        assert_eq!(providers(&super::order(&m, 0)), ["rest-b", "rest-a"]);
        super::succeeded("rest-a");
        assert_eq!(providers(&super::order(&m, 0)), ["rest-a", "rest-b"]);
        // Long after the rest, it is first again even without a success.
        super::failed("rest-a");
        let later = std::time::Instant::now() + super::REST + std::time::Duration::from_secs(1);
        assert_eq!(
            providers(&super::order_at(&m, 0, later)),
            ["rest-a", "rest-b"]
        );
        super::succeeded("rest-a");
    }

    #[test]
    fn only_outages_and_limits_try_another_route() {
        assert!(super::retryable(429) && super::retryable(503) && super::retryable(500));
        assert!(!super::retryable(400) && !super::retryable(401) && !super::retryable(404));
    }
}
