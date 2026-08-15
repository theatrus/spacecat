//! Axum web server for the hub: health, Discord OAuth login, and sessions.

use super::auth::{
    SESSION_COOKIE, cookie_from_header, pkce_challenge, pkce_verifier, sanitize_next_path,
    signed_cookie_value, verify_signed_cookie_value,
};
use super::config::HubConfig;
use super::db::{Db, DbError};
use super::discord_api::{DiscordOauthClient, parse_snowflake, snowflake_string};
use super::store::{GuildSnapshot, SessionRow, UserRow};
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("hub database error: {0}")]
    Db(#[from] DbError),
    #[error("hub I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("hub Discord client error: {0}")]
    Discord(#[from] super::discord_api::DiscordApiError),
}

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct HubState {
    pub db: Db,
    pub config: Arc<HubConfig>,
    /// Present once the Discord application credentials are configured.
    pub oauth: Option<Arc<DiscordOauthClient>>,
}

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/login", get(login))
        .route("/oauth/callback", get(oauth_callback))
        .route("/logout", get(logout))
        .route("/api/session", get(api_session))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(
        "<!doctype html><title>Chatstronomy hub</title>\
         <h1>Chatstronomy hub</h1>\
         <p>space | cat — observatory chat hub. \
         <a href=\"/login\">Log in with Discord</a></p>",
    )
}

/// Liveness and readiness in one: proves the process is up and the database
/// answers a query.
async fn healthz(State(state): State<HubState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.schema_version() {
        Ok(version) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "version": crate::version::VERSION_STRING,
                "schema_version": version,
                "oauth_configured": state.oauth.is_some(),
            })),
        ),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "error",
                "error": e.to_string(),
            })),
        ),
    }
}

#[derive(Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

/// Start the Discord OAuth dance: mint a single-use state row carrying the
/// PKCE verifier and redirect to Discord.
async fn login(State(state): State<HubState>, Query(query): Query<LoginQuery>) -> Response {
    let Some(oauth) = &state.oauth else {
        return service_unavailable("Discord login is not configured on this hub");
    };
    // Opportunistic GC keeps the auth tables from growing without a cron.
    let _ = state.db.cleanup_auth_rows();

    let next = sanitize_next_path(query.next.as_deref().unwrap_or("/"));
    let verifier = pkce_verifier();
    let nonce = match state.db.begin_oauth_state(&verifier, &next) {
        Ok(nonce) => nonce,
        Err(e) => return internal_error(e),
    };
    Redirect::to(&oauth.authorize_url(&nonce, &pkce_challenge(&verifier))).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Finish the OAuth dance: consume the state row, exchange the code, capture
/// the user's identity (including email) and guild snapshot, and mint a
/// session.
async fn oauth_callback(
    State(state): State<HubState>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(oauth) = &state.oauth else {
        return service_unavailable("Discord login is not configured on this hub");
    };
    if let Some(error) = query.error {
        return bad_request(&format!("Discord login was denied: {error}"));
    }
    let (Some(code), Some(nonce)) = (query.code, query.state) else {
        return bad_request("Missing code or state parameter");
    };

    let (verifier, next_path) = match state.db.consume_oauth_state(&nonce) {
        Ok(Some(found)) => found,
        Ok(None) => return bad_request("Login state is unknown, expired, or already used"),
        Err(e) => return internal_error(e),
    };

    let token = match oauth.exchange_code(&code, &verifier).await {
        Ok(token) => token,
        Err(e) => return upstream_error("token exchange", e),
    };
    let discord_user = match oauth.fetch_user(&token.access_token).await {
        Ok(user) => user,
        Err(e) => return upstream_error("user lookup", e),
    };
    let discord_guilds = match oauth.fetch_guilds(&token.access_token).await {
        Ok(guilds) => guilds,
        Err(e) => return upstream_error("guild lookup", e),
    };
    // The access token is dropped here — never persisted.

    let user_id = match parse_snowflake(&discord_user.id) {
        Ok(id) => id,
        Err(e) => return upstream_error("user id", e),
    };
    let user_row = UserRow {
        discord_user_id: user_id,
        username: discord_user.display_name().to_string(),
        email: discord_user.email.clone(),
        email_verified: discord_user.verified,
        avatar_url: discord_user.avatar_url(),
    };
    let guild_rows: Vec<GuildSnapshot> = discord_guilds
        .iter()
        .filter_map(|g| {
            Some(GuildSnapshot {
                guild_id: parse_snowflake(&g.id).ok()?,
                guild_name: g.name.clone(),
                permissions: parse_snowflake(&g.permissions).unwrap_or(0),
                is_owner: g.owner,
            })
        })
        .collect();

    let session = match (|| -> Result<SessionRow, DbError> {
        state.db.upsert_user(&user_row)?;
        state.db.replace_user_guilds(user_id, &guild_rows)?;
        state
            .db
            .create_session(user_id, state.config.session.session_hours)
    })() {
        Ok(session) => session,
        Err(e) => return internal_error(e),
    };

    let cookie = build_session_cookie(
        &state.config,
        &signed_cookie_value(&state.config.session.signing_key, &session.session_id),
        (state.config.session.session_hours * 3600) as i64,
    );
    ([(SET_COOKIE, cookie)], Redirect::to(&next_path)).into_response()
}

/// Delete the session row and clear the cookie.
async fn logout(State(state): State<HubState>, headers: HeaderMap) -> Response {
    if let Some(session) = session_from_headers(&state, &headers) {
        let _ = state.db.delete_session(&session.session_id);
    }
    let cookie = build_session_cookie(&state.config, "", 0);
    ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}

/// Who am I? Hands the SPA the CSRF token for mutating requests.
async fn api_session(State(state): State<HubState>, headers: HeaderMap) -> Response {
    let Some(session) = session_from_headers(&state, &headers) else {
        return Json(serde_json::json!({ "authenticated": false })).into_response();
    };
    let user = match state.db.get_user(session.discord_user_id) {
        Ok(Some(user)) => user,
        Ok(None) => return Json(serde_json::json!({ "authenticated": false })).into_response(),
        Err(e) => return internal_error(e),
    };
    Json(serde_json::json!({
        "authenticated": true,
        "csrf_token": session.csrf_token,
        "user": {
            "id": snowflake_string(user.discord_user_id),
            "username": user.username,
            "email": user.email,
            "email_verified": user.email_verified,
            "avatar_url": user.avatar_url,
        },
    }))
    .into_response()
}

/// Resolve the current session from request headers: parse the cookie,
/// verify its signature, then load the row (expiry enforced there).
pub fn session_from_headers(state: &HubState, headers: &HeaderMap) -> Option<SessionRow> {
    let header = headers.get(COOKIE)?.to_str().ok()?;
    let value = cookie_from_header(header, SESSION_COOKIE)?;
    let session_id = verify_signed_cookie_value(&state.config.session.signing_key, &value)?;
    state.db.get_session(&session_id).ok().flatten()
}

/// Require a valid session plus a matching `x-csrf-token` header. For the
/// mutating endpoints arriving in later phases.
pub fn require_session_with_csrf(state: &HubState, headers: &HeaderMap) -> Option<SessionRow> {
    let session = session_from_headers(state, headers)?;
    let provided = headers.get("x-csrf-token")?.to_str().ok()?;
    use subtle::ConstantTimeEq;
    bool::from(provided.as_bytes().ct_eq(session.csrf_token.as_bytes())).then_some(session)
}

fn build_session_cookie(config: &HubConfig, value: &str, max_age: i64) -> String {
    let secure = if config.public_base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    format!("{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, message.to_string()).into_response()
}

fn service_unavailable(message: &str) -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, message.to_string()).into_response()
}

fn internal_error(e: impl std::fmt::Display) -> Response {
    eprintln!("Hub internal error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error".to_string(),
    )
        .into_response()
}

fn upstream_error(stage: &str, e: impl std::fmt::Display) -> Response {
    eprintln!("Hub Discord {stage} failed: {e}");
    (StatusCode::BAD_GATEWAY, format!("Discord {stage} failed")).into_response()
}

/// Open the database, bind, and serve until ctrl-c.
pub async fn run(config: HubConfig) -> Result<(), HubError> {
    let db = Db::open(&config.database_path)?;
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!(
        "Hub listening on http://{} (database: {})",
        listener.local_addr()?,
        config.database_path
    );
    serve(listener, config, db).await
}

/// Serve on an already-bound listener. Split from `run` so tests can bind
/// port 0 and use an in-memory database.
pub async fn serve(
    listener: tokio::net::TcpListener,
    config: HubConfig,
    db: Db,
) -> Result<(), HubError> {
    let oauth = if config.oauth_configured() {
        Some(Arc::new(DiscordOauthClient::new(
            &config.discord.base_url,
            &config.discord.client_id,
            &config.discord.client_secret,
            &config.public_base_url,
        )?))
    } else {
        println!("Discord login not configured; web login is disabled");
        None
    };
    let state = HubState {
        db,
        config: Arc::new(config),
        oauth,
    };
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    println!("Hub shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Form;

    async fn spawn_hub(config: HubConfig) -> (String, Db) {
        let db = Db::open_in_memory().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, config, db.clone()));
        (format!("http://{addr}"), db)
    }

    async fn spawn_test_hub() -> String {
        spawn_hub(HubConfig::default()).await.0
    }

    #[tokio::test]
    async fn healthz_reports_ok_and_schema_version() {
        let base = spawn_test_hub().await;
        let response = reqwest::get(format!("{base}/healthz")).await.unwrap();
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["schema_version"].as_u64().unwrap() >= 2);
        assert_eq!(body["oauth_configured"], false);
    }

    #[tokio::test]
    async fn index_serves_html() {
        let base = spawn_test_hub().await;
        let response = reqwest::get(&base).await.unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.text().await.unwrap().contains("Chatstronomy hub"));
    }

    #[tokio::test]
    async fn login_unconfigured_returns_503() {
        let base = spawn_test_hub().await;
        let response = reqwest::get(format!("{base}/login")).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn session_endpoint_unauthenticated() {
        let base = spawn_test_hub().await;
        let body: serde_json::Value = reqwest::get(format!("{base}/api/session"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(body["authenticated"], false);
    }

    /// A stub of the three Discord endpoints the login flow calls.
    async fn spawn_stub_discord() -> String {
        #[derive(Deserialize)]
        struct TokenForm {
            code: String,
            code_verifier: String,
            grant_type: String,
        }
        async fn token(Form(form): Form<TokenForm>) -> Response {
            if form.grant_type != "authorization_code"
                || form.code != "test-code"
                || form.code_verifier.is_empty()
            {
                return (StatusCode::BAD_REQUEST, "bad token request").into_response();
            }
            Json(serde_json::json!({
                "access_token": "stub-access-token",
                "token_type": "Bearer"
            }))
            .into_response()
        }
        fn authed(headers: &HeaderMap) -> bool {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v == "Bearer stub-access-token")
        }
        async fn me(headers: HeaderMap) -> Response {
            if !authed(&headers) {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Json(serde_json::json!({
                "id": "80351110224678912",
                "username": "nelly",
                "global_name": "Nelly",
                "avatar": "abc123",
                "email": "nelly@example.com",
                "verified": true
            }))
            .into_response()
        }
        async fn guilds(headers: HeaderMap) -> Response {
            if !authed(&headers) {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Json(serde_json::json!([
                {"id": "197038439483310086", "name": "Observatory", "owner": true,
                 "permissions": "2147483647"},
                {"id": "300", "name": "Other", "owner": false, "permissions": "0"}
            ]))
            .into_response()
        }
        let app = Router::new()
            .route("/api/oauth2/token", axum::routing::post(token))
            .route("/api/users/@me", get(me))
            .route("/api/users/@me/guilds", get(guilds));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn oauth_config(stub_base: &str) -> HubConfig {
        let mut config = HubConfig::default();
        config.discord.base_url = stub_base.to_string();
        config.discord.client_id = "client-1".to_string();
        config.discord.client_secret = "secret-1".to_string();
        config.public_base_url = "http://hub.test".to_string();
        config.session.signing_key = "0123456789abcdef0123456789abcdef".to_string();
        config
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .cookie_store(true)
            .build()
            .unwrap()
    }

    /// Drive /login and return the state nonce Discord would echo back.
    async fn login_state(client: &reqwest::Client, base: &str) -> String {
        let response = client
            .get(format!("{base}/login?next=/after-login"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 303);
        let location = response.headers()["location"].to_str().unwrap();
        assert!(location.contains("/oauth2/authorize"));
        assert!(location.contains("code_challenge_method=S256"));
        let url = url::Url::parse(location).unwrap();
        url.query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .unwrap()
    }

    #[tokio::test]
    async fn full_login_flow_captures_email_and_guilds() {
        let stub = spawn_stub_discord().await;
        let (base, db) = spawn_hub(oauth_config(&stub)).await;
        let client = client();

        let state = login_state(&client, &base).await;
        let response = client
            .get(format!(
                "{base}/oauth/callback?code=test-code&state={state}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers()["location"], "/after-login");
        assert!(response.headers().contains_key("set-cookie"));

        // Session endpoint sees the user, with email captured.
        let body: serde_json::Value = client
            .get(format!("{base}/api/session"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(body["authenticated"], true);
        assert_eq!(body["user"]["email"], "nelly@example.com");
        assert_eq!(body["user"]["username"], "Nelly");
        assert_eq!(body["user"]["id"], "80351110224678912");
        assert!(body["csrf_token"].is_string());

        // Guild snapshot with permissions landed in the database.
        let guilds = db.user_guilds(80351110224678912).unwrap();
        assert_eq!(guilds.len(), 2);
        let observatory = guilds
            .iter()
            .find(|g| g.guild_name == "Observatory")
            .unwrap();
        assert!(observatory.is_owner);
        assert!(crate::hub::auth::can_manage_guild(
            observatory.permissions,
            observatory.is_owner
        ));
    }

    #[tokio::test]
    async fn oauth_state_cannot_be_replayed() {
        let stub = spawn_stub_discord().await;
        let (base, _db) = spawn_hub(oauth_config(&stub)).await;
        let client = client();

        let state = login_state(&client, &base).await;
        let first = client
            .get(format!(
                "{base}/oauth/callback?code=test-code&state={state}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(first.status(), 303);
        let replay = client
            .get(format!(
                "{base}/oauth/callback?code=test-code&state={state}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(replay.status(), 400);
    }

    #[tokio::test]
    async fn callback_with_unknown_state_rejected() {
        let stub = spawn_stub_discord().await;
        let (base, _db) = spawn_hub(oauth_config(&stub)).await;
        let response = reqwest::get(format!(
            "{base}/oauth/callback?code=test-code&state=made-up"
        ))
        .await
        .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn logout_ends_session() {
        let stub = spawn_stub_discord().await;
        let (base, _db) = spawn_hub(oauth_config(&stub)).await;
        let client = client();

        let state = login_state(&client, &base).await;
        client
            .get(format!(
                "{base}/oauth/callback?code=test-code&state={state}"
            ))
            .send()
            .await
            .unwrap();
        let response = client.get(format!("{base}/logout")).send().await.unwrap();
        assert_eq!(response.status(), 303);

        let body: serde_json::Value = client
            .get(format!("{base}/api/session"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(body["authenticated"], false);
    }

    #[tokio::test]
    async fn forged_cookie_rejected() {
        let stub = spawn_stub_discord().await;
        let (base, db) = spawn_hub(oauth_config(&stub)).await;
        // A session row exists, but the cookie is signed with the wrong key.
        db.upsert_user(&UserRow {
            discord_user_id: 1,
            username: "u".to_string(),
            email: None,
            email_verified: false,
            avatar_url: None,
        })
        .unwrap();
        let session = db.create_session(1, 1).unwrap();
        let forged = signed_cookie_value("wrong-key-wrong-key-wrong-key-00", &session.session_id);

        let body: serde_json::Value = reqwest::Client::new()
            .get(format!("{base}/api/session"))
            .header("cookie", format!("{SESSION_COOKIE}={forged}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(body["authenticated"], false);
    }
}
