//! The model catalog (which models Cuckoo may call, at what price) and routes
//! (which provider serves a model agents ask for, the split and the backup).
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub provider: String,
    pub model: String,
}

/// How the gateway serves one requested model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Route {
    pub model: String,
    pub main: Target,
    pub second: Option<Target>,
    pub main_percent: i32,
    pub timeout_seconds: i32,
}

pub(crate) fn model_name(text: &str) -> Result<String> {
    let text = text.trim();
    ensure!(!text.is_empty(), "Name the model");
    one_line(text, 200, "model name")?;
    ensure!(
        !text.contains(char::is_whitespace),
        "Model names have no spaces"
    );
    Ok(text.to_owned())
}

impl Route {
    pub(crate) fn validate(&self) -> Result<()> {
        model_name(&self.model)?;
        for target in std::iter::once(&self.main).chain(self.second.as_ref()) {
            providers::name(&target.provider)?;
            model_name(&target.model)?;
        }
        ensure!(
            (0..=100).contains(&self.main_percent),
            "The split is a percentage from 0 to 100"
        );
        ensure!(
            self.second.is_some() || self.main_percent == 100,
            "Without a second model, the main model takes all traffic"
        );
        ensure!(
            self.second.as_ref() != Some(&self.main),
            "Choose a different second model"
        );
        ensure!(
            (5..=600).contains(&self.timeout_seconds),
            "Use a timeout from 5 to 600 seconds"
        );
        Ok(())
    }
    pub(crate) fn describe(&self) -> String {
        let main = format!("{}:{}", self.main.provider, self.main.model);
        match &self.second {
            Some(second) => format!(
                "{} → {}% {main}, {}% {}:{} (backup), timeout {} s",
                self.model,
                self.main_percent,
                100 - self.main_percent,
                second.provider,
                second.model,
                self.timeout_seconds
            ),
            None => format!(
                "{} → 100% {main}, no backup, timeout {} s",
                self.model, self.timeout_seconds
            ),
        }
    }
}

fn route_from(row: &QueryResult) -> Result<Route> {
    let second_provider: Option<String> = row.try_get("", "second_provider")?;
    let second_model: Option<String> = row.try_get("", "second_model")?;
    Ok(Route {
        model: row.try_get("", "model")?,
        main: Target {
            provider: row.try_get("", "main_provider")?,
            model: row.try_get("", "main_model")?,
        },
        second: second_provider
            .zip(second_model)
            .map(|(provider, model)| Target { provider, model }),
        main_percent: row.try_get("", "main_percent")?,
        timeout_seconds: row.try_get("", "timeout_seconds")?,
    })
}

impl PgStore {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn operator_set_model(
        &self,
        by: &Operator,
        provider: &str,
        model: &str,
        enabled: bool,
        input_usd_per_mtok: f64,
        output_usd_per_mtok: f64,
        why: &str,
    ) -> Result<()> {
        let (input, output) = (micros(input_usd_per_mtok)?, micros(output_usd_per_mtok)?);
        let tx = self.db.begin().await?;
        execute(&tx, "INSERT INTO relay_platform_models(provider,model,enabled,input_micros_per_mtok,output_micros_per_mtok,changed_by,changed_ms)
            VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(provider,model) DO UPDATE SET enabled=excluded.enabled,
            input_micros_per_mtok=excluded.input_micros_per_mtok,output_micros_per_mtok=excluded.output_micros_per_mtok,
            changed_by=excluded.changed_by,changed_ms=excluded.changed_ms",
            vec![provider.into(), model.into(), enabled.into(), input.into(), output.into(), by.user.id.clone().into(), (now() * 1000).into()]).await?;
        let state = if enabled { "allowed" } else { "blocked" };
        record(&tx, by, "models", "model_set", &format!("{provider}:{model}"),
            &format!("{state} {provider}:{model} at ${input_usd_per_mtok} in / ${output_usd_per_mtok} out per 1M tokens"), why).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Whether the catalog blocks this model; a model with no entry is allowed.
    pub(crate) async fn model_blocked(&self, provider: &str, model: &str) -> Result<bool> {
        Ok(query(&self.db, "SELECT NOT enabled AS blocked FROM relay_platform_models WHERE provider=$1 AND model=$2",
            vec![provider.into(), model.into()]).await?
            .map(|row| row.try_get::<bool>("", "blocked")).transpose()?.unwrap_or(false))
    }

    pub(crate) async fn model_route(&self, model: &str) -> Result<Option<Route>> {
        query(
            &self.db,
            "SELECT * FROM relay_model_routes WHERE model=$1",
            vec![model.into()],
        )
        .await?
        .map(|row| route_from(&row))
        .transpose()
    }

    pub(crate) async fn operator_set_route(
        &self,
        by: &Operator,
        route: &Route,
        why: &str,
    ) -> Result<()> {
        route.validate()?;
        for target in std::iter::once(&route.main).chain(route.second.as_ref()) {
            ensure!(
                !self.model_blocked(&target.provider, &target.model).await?,
                "{}:{} is blocked in the model catalog",
                target.provider,
                target.model
            );
        }
        let tx = self.db.begin().await?;
        let before = query(
            &tx,
            "SELECT * FROM relay_model_routes WHERE model=$1 FOR UPDATE",
            vec![route.model.clone().into()],
        )
        .await?
        .map(|row| route_from(&row))
        .transpose()?;
        execute(&tx, "INSERT INTO relay_model_routes(model,main_provider,main_model,second_provider,second_model,main_percent,timeout_seconds,changed_by,changed_ms)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(model) DO UPDATE SET main_provider=excluded.main_provider,main_model=excluded.main_model,
            second_provider=excluded.second_provider,second_model=excluded.second_model,main_percent=excluded.main_percent,
            timeout_seconds=excluded.timeout_seconds,changed_by=excluded.changed_by,changed_ms=excluded.changed_ms",
            vec![route.model.clone().into(), route.main.provider.clone().into(), route.main.model.clone().into(),
                route.second.as_ref().map(|t| t.provider.clone()).into(), route.second.as_ref().map(|t| t.model.clone()).into(),
                route.main_percent.into(), route.timeout_seconds.into(), by.user.id.clone().into(), (now() * 1000).into()]).await?;
        let summary = match before {
            Some(old) if old == *route => format!("saved route {} unchanged", route.describe()),
            Some(old) => format!(
                "changed route {} (was {})",
                route.describe(),
                old.describe()
            ),
            None => format!("added route {}", route.describe()),
        };
        record(&tx, by, "routing", "route_set", &route.model, &summary, why).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn operator_remove_route(
        &self,
        by: &Operator,
        model: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let removed = execute(
            &tx,
            "DELETE FROM relay_model_routes WHERE model=$1",
            vec![model.into()],
        )
        .await?;
        ensure!(removed == 1, "No route for {model}");
        record(
            &tx,
            by,
            "routing",
            "route_removed",
            model,
            &format!(
                "removed the route for {model}; agents asking for it use their own provider again"
            ),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Routes with their last hour of traffic, and the models agents use today.
    pub(crate) async fn operator_routes(&self) -> Result<Value> {
        let hour_ago = now() * 1000 - 3_600_000;
        let routes = json_rows(&self.db, "SELECT jsonb_build_object('model',r.model,'main_provider',r.main_provider,'main_model',r.main_model,
            'second_provider',r.second_provider,'second_model',r.second_model,'main_percent',r.main_percent,'timeout_seconds',r.timeout_seconds,
            'changed_ms',r.changed_ms,'by',COALESCE(u.display_name,u.email),
            'traffic',(SELECT COALESCE(jsonb_agg(jsonb_build_object('provider',t.provider,'model',t.model,'calls',t.calls,'errors',t.errors,'p95_ms',t.p95)),'[]'::jsonb)
              FROM (SELECT provider,model,count(*) AS calls,count(*) FILTER (WHERE outcome='error') AS errors,
                (percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FILTER (WHERE outcome='ok'))::bigint AS p95
                FROM relay_model_usage WHERE requested_model=r.model AND at_ms>=$1 GROUP BY provider,model) t)) AS data
            FROM relay_model_routes r JOIN relay_users u ON u.id=r.changed_by ORDER BY r.model", vec![hour_ago.into()]).await?;
        let requested = json_rows(&self.db, "SELECT jsonb_build_object('model',m.model,'agents',sum(m.agents),'calls_7d',sum(m.calls)) AS data FROM (
              SELECT model,count(*) AS agents,0 AS calls FROM relay_shared_agents WHERE model IS NOT NULL AND model<>'' GROUP BY model
              UNION ALL SELECT COALESCE(requested_model,model),0,count(*) FROM relay_model_usage WHERE at_ms>=$1 GROUP BY 1) m
            GROUP BY m.model ORDER BY sum(m.calls) DESC, m.model LIMIT 100", vec![(now() * 1000 - 7 * 86_400_000).into()]).await?;
        Ok(json!({"routes": routes, "requested": requested}))
    }
}
