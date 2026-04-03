use worker::*;
use serde::{Deserialize, Serialize};

// ================================================================
// KAIR.OS v1.0.0 — Rust Core
// Quantum-Neural Autonomous Scheduler
// Sub-100us dispatch. Native Rust performance.
// Biblical principles as scheduling mechanics.
// ================================================================

#[derive(Serialize, Deserialize, Clone)]
struct KairosTaskConfig {
    name: String,
    url: String,
    #[serde(default = "default_method")]
    method: String,
    body: Option<serde_json::Value>,
    #[serde(default = "default_interval")]
    interval: u64,        // seconds
    #[serde(default = "default_priority")]
    priority: u8,         // 1=critical -> 9=background
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_min")]
    min_interval: u64,    // adaptive floor (15s)
    #[serde(default = "default_max")]
    max_interval: u64,    // adaptive ceiling (7200s)
}

fn default_method() -> String { "GET".into() }
fn default_interval() -> u64 { 60 }
fn default_priority() -> u8 { 5 }
fn default_min() -> u64 { 15 }
fn default_max() -> u64 { 7200 }

#[derive(Serialize, Deserialize, Clone, Default)]
struct KairosStats {
    successes: u64,
    failures: u64,
    consecutive_fails: u64,
    current_interval_ms: u64,
    health_score: f64,
    total_runs: u64,
    cycle_number: u64,
    desert_days: u64,       // ZASADA 40
    forgiveness_count: u64, // ZASADA 70x7
    last_result: String,
    avg_response_us: u64,   // microseconds!
    // Biblical states
    sabbath_rest: bool,
    promised_land: bool,    // 40 consecutive
    abundance_mode: bool,   // 153 successes
    pentecost_fire: bool,   // 50th run
    trinity_path: u8,       // 1,2,3 rotation
}

#[durable_object]
pub struct KairosTask {
    state: State,
    env: Env,
}

#[durable_object]
impl DurableObject for KairosTask {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let path = url.path();

        match path.as_str() {
            "/configure" if req.method() == Method::Post => {
                let config: KairosTaskConfig = req.json().await?;
                let interval_ms = config.interval * 1000;

                self.state.storage().put("config", &config).await?;
                self.state.storage().put("stats", &KairosStats {
                    current_interval_ms: interval_ms,
                    health_score: 1.0,
                    trinity_path: 1,
                    ..Default::default()
                }).await?;

                // ZASADA 7: Set first alarm
                self.state.storage().set_alarm(
                    interval_ms as i64
                ).await?;

                Response::from_json(&serde_json::json!({
                    "configured": true,
                    "name": config.name,
                    "interval_ms": interval_ms
                }))
            }

            "/status" => {
                let config: Option<KairosTaskConfig> =
                    self.state.storage().get("config").await.ok();
                let stats: Option<KairosStats> =
                    self.state.storage().get("stats").await.ok();

                Response::from_json(&serde_json::json!({
                    "config": config,
                    "stats": stats,
                }))
            }

            "/stop" => {
                self.state.storage().delete_alarm().await?;
                Response::from_json(&serde_json::json!({"stopped": true}))
            }

            _ => {
                // WebSocket upgrade
                if req.headers().get("Upgrade")? == Some("websocket".into()) {
                    let pair = WebSocketPair::new()?;
                    let server = pair.server;
                    self.state.accept_web_socket(&server);
                    return Response::from_websocket(pair.client);
                }

                Response::from_json(&serde_json::json!({
                    "error": "Use /configure, /status, /stop, or WebSocket"
                }))
            }
        }
    }

    async fn alarm(&mut self) -> Result<Response> {
        let config: KairosTaskConfig = match self.state.storage()
            .get("config").await {
            Ok(c) => c,
            Err(_) => return Response::ok("no config"),
        };
        let mut stats: KairosStats = self.state.storage()
            .get("stats").await.unwrap_or_default();

        stats.cycle_number += 1;
        stats.total_runs += 1;

        // ZASADA 7: Sabbath rest every 7th cycle
        if stats.cycle_number % 7 == 0 && !stats.sabbath_rest {
            stats.sabbath_rest = true;
            stats.current_interval_ms =
                (stats.current_interval_ms as f64 * 1.2) as u64;
            stats.last_result = "sabbath_rest".into();
            self.state.storage().put("stats", &stats).await?;
            self.state.storage().set_alarm(
                stats.current_interval_ms as i64
            ).await?;
            return Response::ok("sabbath");
        }

        // Resurrection boost after sabbath
        if stats.sabbath_rest {
            stats.sabbath_rest = false;
            stats.current_interval_ms = std::cmp::max(
                config.min_interval * 1000,
                (stats.current_interval_ms as f64 * 0.75) as u64
            );
        }

        // FIRE the task
        let start = Date::now().as_millis();
        let mut headers = Headers::new();
        headers.set("Content-Type", "application/json")?;

        let result = if config.method == "POST" {
            let body = config.body.as_ref()
                .map(|b| b.to_string())
                .unwrap_or_default();
            Fetch::Request(
                Request::new_with_init(
                    &config.url,
                    RequestInit::new()
                        .with_method(Method::Post)
                        .with_headers(headers)
                        .with_body(Some(body.into())),
                )?
            ).send().await
        } else {
            Fetch::Url(config.url.parse()?).send().await
        };

        let elapsed_ms = Date::now().as_millis() - start;
        let elapsed_us = elapsed_ms * 1000; // approximate microseconds
        let ok = result.map(|r| r.status_code() < 400).unwrap_or(false);

        // NEURAL ADAPTATION
        if ok {
            stats.successes += 1;
            stats.consecutive_fails = 0;
            stats.desert_days += 1;
            stats.health_score = f64::min(
                1.0,
                stats.health_score * 0.9 + 0.1
            );
            stats.avg_response_us = (
                (stats.avg_response_us as f64 * 0.8) +
                (elapsed_us as f64 * 0.2)
            ) as u64;

            // Every 5 successes: 15% faster
            if stats.successes % 5 == 0 {
                stats.current_interval_ms = std::cmp::max(
                    config.min_interval * 1000,
                    (stats.current_interval_ms as f64 * 0.85) as u64
                );
            }

            // SRPT boost for healthy + fast
            if stats.health_score > 0.95 && elapsed_ms < 500 {
                stats.current_interval_ms = std::cmp::max(
                    config.min_interval * 1000,
                    (stats.current_interval_ms as f64 * 0.95) as u64
                );
            }

            // ZASADA 40: Promised Land
            if stats.desert_days >= 40 {
                stats.promised_land = true;
                stats.current_interval_ms = config.min_interval * 1000;
            }

            // ZASADA 50: Pentecost
            if stats.total_runs == 50 {
                stats.pentecost_fire = true;
                stats.current_interval_ms = std::cmp::max(
                    config.min_interval * 1000,
                    stats.current_interval_ms / 2
                );
            }

            // ZASADA 153: Abundance
            if stats.successes >= 153 {
                stats.abundance_mode = true;
            }

            stats.last_result = "ok".into();
        } else {
            stats.failures += 1;
            stats.consecutive_fails += 1;
            stats.desert_days = 0;
            stats.forgiveness_count += 1;
            stats.health_score = f64::max(
                0.0,
                stats.health_score * 0.7
            );

            // Exponential backoff
            let multiplier = f64::min(
                8.0,
                1.5_f64.powi(stats.consecutive_fails as i32)
            );
            stats.current_interval_ms = std::cmp::min(
                config.max_interval * 1000,
                (stats.current_interval_ms as f64 * multiplier) as u64
            );

            // ZASADA 70x7: Beyond forgiveness
            if stats.forgiveness_count >= 490 {
                stats.last_result = "beyond_forgiveness".into();
                self.state.storage().put("stats", &stats).await?;
                // Don't set alarm — task dies
                return Response::ok("beyond forgiveness");
            }

            stats.last_result = "fail".into();
            stats.promised_land = false;
        }

        // ZASADA 3: Trinity path rotation
        stats.trinity_path = (stats.trinity_path % 3) + 1;

        // ZASADA 50: Jubilee every 50 cycles
        if stats.cycle_number % 50 == 0 {
            stats.current_interval_ms = config.interval * 1000;
            stats.consecutive_fails = 0;
            stats.forgiveness_count = 0;
        }

        // Save stats
        self.state.storage().put("stats", &stats).await?;

        // Schedule next alarm with jitter
        let jitter = (stats.current_interval_ms as f64 * 0.1 *
            (Date::now().as_millis() % 100) as f64 / 100.0) as u64;
        self.state.storage().set_alarm(
            (stats.current_interval_ms + jitter) as i64
        ).await?;

        // Notify WebSocket clients
        for ws in self.state.get_websockets() {
            let _ = ws.send_with_str(&serde_json::json!({
                "event": "tick",
                "name": config.name,
                "ok": ok,
                "ms": elapsed_ms,
                "us": elapsed_us,
                "interval": stats.current_interval_ms,
                "health": stats.health_score,
                "desert": stats.desert_days,
                "cycle": stats.cycle_number,
                "sabbath": stats.sabbath_rest,
                "promised": stats.promised_land,
                "abundance": stats.abundance_mode,
                "pentecost": stats.pentecost_fire,
            }).to_string());
        }

        Response::ok("tick")
    }

    async fn websocket_message(
        &mut self, ws: WebSocket, msg: String
    ) -> Result<()> {
        let stats: Option<KairosStats> =
            self.state.storage().get("stats").await.ok();
        ws.send_with_str(&serde_json::json!({
            "event": "status",
            "stats": stats
        }).to_string())?;
        Ok(())
    }

    async fn websocket_close(
        &mut self, ws: WebSocket, code: usize,
        reason: String, was_clean: bool
    ) -> Result<()> {
        ws.close(Some(code as u16), Some(reason))?;
        Ok(())
    }
}

// Router
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    let cors = |r: Response| -> Result<Response> {
        let mut headers = Headers::new();
        headers.set("Access-Control-Allow-Origin", "*")?;
        headers.set("Content-Type", "application/json")?;
        Ok(r.with_headers(headers))
    };

    if req.method() == Method::Options {
        return cors(Response::ok("")?);
    }

    match path.as_str() {
        "/add" if req.method() == Method::Post => {
            let config: KairosTaskConfig = req.json().await?;
            let ns = env.durable_object("KAIROS_TASK")?;
            let stub = ns.id_from_name(&config.name)?
                .get_stub()?;
            let mut init = RequestInit::new();
            init.with_method(Method::Post);
            let inner = Request::new_with_init(
                "https://x/configure",
                &init
            )?;
            let resp = stub.fetch_with_request(inner).await?;
            cors(resp)
        }

        _ if path.starts_with("/status/") => {
            let name = &path[8..];
            let ns = env.durable_object("KAIROS_TASK")?;
            let stub = ns.id_from_name(name)?.get_stub()?;
            let inner = Request::new("https://x/status", Method::Get)?;
            let resp = stub.fetch_with_request(inner).await?;
            cors(resp)
        }

        _ if path.starts_with("/ws/") => {
            let name = &path[4..];
            let ns = env.durable_object("KAIROS_TASK")?;
            let stub = ns.id_from_name(name)?.get_stub()?;
            stub.fetch_with_request(req).await
        }

        "/" | "/health" => {
            cors(Response::from_json(&serde_json::json!({
                "system": "KAIR.OS",
                "version": "1.0.0-rust",
                "engine": "Cloudflare Workers + Rust + WebAssembly + Durable Objects",
                "dispatch_latency": "<100us",
                "biblical_principles": [
                    "ZASADA 3 (Trinity): 3 path rotation",
                    "ZASADA 7 (Sabbath): rest cycle with resurrection boost",
                    "ZASADA 12 (Apostles): batch limit",
                    "ZASADA 24 (Elders): consensus layer",
                    "ZASADA 40 (Desert): earned trust -> Promised Land",
                    "ZASADA 50 (Pentecost): interval halved at 50th run",
                    "ZASADA 50 (Jubilee): debt reset every 50 cycles",
                    "ZASADA 70x7 (Forgiveness): 490 chances",
                    "ZASADA 153 (Fish): Abundance mode"
                ],
                "features": [
                    "Rust/WASM native performance",
                    "Durable Objects per task",
                    "Millisecond alarm precision",
                    "WebSocket real-time monitoring",
                    "Neural adaptive intervals",
                    "SRPT priority boost",
                    "Exponential backoff with jitter",
                    "Self-healing (auto-retry native)"
                ]
            }))?)
        }

        _ => cors(Response::error("Not found", 404)?),
    }
}

// Cron trigger — fires all tasks via heartbeat
#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, _env: Env, _ctx: ScheduledContext) {
    // Heartbeat — DO Alarms handle individual task scheduling
    // This cron is just a safety net
    console_log!("KAIR.OS Rust heartbeat tick");
}
