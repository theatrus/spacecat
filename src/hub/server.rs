//! Axum web server for the hub: health, Discord OAuth login, and sessions.

use super::auth::{
    SESSION_COOKIE, cookie_from_header, pkce_challenge, pkce_verifier, sanitize_next_path,
    signed_cookie_value, verify_signed_cookie_value,
};
use super::config::HubConfig;
use super::db::{Db, DbError};
use super::discord_api::{DiscordOauthClient, parse_snowflake, snowflake_string};
use super::guild_check::{CachedGuildChecker, GuildChecker, SerenityGuildChecker};
use super::store::{GuildSnapshot, SessionRow, UserRow};
use super::tenants::{TelescopeRow, TelescopeUpdate};
use axum::extract::{Path, Query, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
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
    #[error("hub chat error: {0}")]
    Chat(#[from] crate::error::ChatError),
}

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct HubState {
    pub db: Db,
    pub config: Arc<HubConfig>,
    /// Present once the Discord application credentials are configured.
    pub oauth: Option<Arc<DiscordOauthClient>>,
    /// Present once a bot token is configured; backs live guild checks.
    pub guild_checker: Option<Arc<dyn GuildChecker>>,
    /// Live rig connections from `/v1/direct`.
    pub rig_connections: Arc<super::direct_server::RigConnections>,
}

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/login", get(login))
        .route("/oauth/callback", get(oauth_callback))
        .route("/logout", get(logout))
        .route("/api/session", get(api_session))
        .route("/api/guilds", get(api_list_guilds))
        .route("/api/guilds/{guild_id}/register", post(api_register_guild))
        .route(
            "/api/guilds/{guild_id}/telescopes",
            get(api_list_telescopes).post(api_create_telescope),
        )
        .route(
            "/api/telescopes/{telescope_id}",
            axum::routing::patch(api_update_telescope).delete(api_delete_telescope),
        )
        .route(
            "/api/telescopes/{telescope_id}/pairing-token",
            post(api_issue_pairing_token),
        )
        .route(
            "/api/telescopes/{telescope_id}/pairing-tokens",
            delete(api_revoke_pairing_tokens),
        )
        .route(
            crate::direct::protocol::DIRECT_WEBSOCKET_PATH,
            get(super::direct_server::direct_ws),
        )
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(super::web_ui::INDEX_HTML)
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

// ---------------------------------------------------------------------------
// Guild and telescope management API
// ---------------------------------------------------------------------------

/// Authorization outcome for managing a guild.
enum ManageAuth {
    Ok(SessionRow),
    Denied(Response),
}

/// Authorize a management action on a guild: a session (with CSRF for
/// mutations), MANAGE_GUILD/owner in the OAuth snapshot, and — when a bot
/// token is configured — a live membership check so a user who left the
/// guild loses access before their snapshot refreshes.
async fn authorize_manage(
    state: &HubState,
    headers: &HeaderMap,
    guild_id: i64,
    mutating: bool,
) -> ManageAuth {
    let session = if mutating {
        match require_session_with_csrf(state, headers) {
            Some(session) => session,
            None => {
                return ManageAuth::Denied(
                    (StatusCode::UNAUTHORIZED, "login and CSRF token required").into_response(),
                );
            }
        }
    } else {
        match session_from_headers(state, headers) {
            Some(session) => session,
            None => {
                return ManageAuth::Denied(
                    (StatusCode::UNAUTHORIZED, "login required").into_response(),
                );
            }
        }
    };

    let snapshot = match state.db.user_guilds(session.discord_user_id) {
        Ok(guilds) => guilds,
        Err(e) => return ManageAuth::Denied(internal_error(e)),
    };
    let can_manage = snapshot.iter().any(|g| {
        g.guild_id == guild_id && super::auth::can_manage_guild(g.permissions, g.is_owner)
    });
    if !can_manage {
        return ManageAuth::Denied(
            (StatusCode::FORBIDDEN, "not a manager of this guild").into_response(),
        );
    }

    if let Some(checker) = &state.guild_checker
        && !checker
            .user_in_guild(guild_id as u64, session.discord_user_id as u64)
            .await
    {
        return ManageAuth::Denied(
            (StatusCode::FORBIDDEN, "no longer a member of this guild").into_response(),
        );
    }

    ManageAuth::Ok(session)
}

#[allow(clippy::result_large_err)] // Err is the ready-to-send error Response
fn parse_id_param(raw: &str) -> Result<i64, Response> {
    parse_snowflake(raw).map_err(|_| bad_request("invalid id"))
}

fn telescope_json(t: &TelescopeRow) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "guild_id": snowflake_string(t.guild_id),
        "name": t.name,
        "discord_channel_id": t.discord_channel_id.map(snowflake_string),
        "image_cooldown_seconds": t.image_cooldown_seconds,
        "write_policy": t.write_policy,
        "allowed_role_ids": t.allowed_role_ids.iter().copied()
            .map(snowflake_string).collect::<Vec<_>>(),
    })
}

/// The guilds this user can manage, with registration and bot state.
async fn api_list_guilds(State(state): State<HubState>, headers: HeaderMap) -> Response {
    let Some(session) = session_from_headers(&state, &headers) else {
        return (StatusCode::UNAUTHORIZED, "login required").into_response();
    };
    let snapshot = match state.db.user_guilds(session.discord_user_id) {
        Ok(guilds) => guilds,
        Err(e) => return internal_error(e),
    };
    let mut out = Vec::new();
    for g in snapshot
        .iter()
        .filter(|g| super::auth::can_manage_guild(g.permissions, g.is_owner))
    {
        let registered = match state.db.get_guild(g.guild_id) {
            Ok(row) => row.is_some(),
            Err(e) => return internal_error(e),
        };
        let bot_installed = match &state.guild_checker {
            Some(checker) => serde_json::Value::from(checker.bot_in_guild(g.guild_id as u64).await),
            None => serde_json::Value::Null,
        };
        // The Discord app-install link for this guild. The client ID is
        // public, so exposing it here is fine.
        let install_url = if state.config.discord.client_id.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::from(format!(
                "{}/oauth2/authorize?client_id={}&scope=bot+applications.commands&guild_id={}",
                state.config.discord.base_url.trim_end_matches('/'),
                state.config.discord.client_id,
                snowflake_string(g.guild_id),
            ))
        };
        out.push(serde_json::json!({
            "id": snowflake_string(g.guild_id),
            "name": g.guild_name,
            "registered": registered,
            "bot_installed": bot_installed,
            "install_url": install_url,
        }));
    }
    Json(serde_json::json!({ "guilds": out })).into_response()
}

/// Register a guild as a tenant. Requires the bot to be installed when a
/// live checker is available.
async fn api_register_guild(
    State(state): State<HubState>,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let guild_id = match parse_id_param(&guild_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let session = match authorize_manage(&state, &headers, guild_id, true).await {
        ManageAuth::Ok(session) => session,
        ManageAuth::Denied(response) => return response,
    };
    if let Some(checker) = &state.guild_checker
        && !checker.bot_in_guild(guild_id as u64).await
    {
        return bad_request("install the Chatstronomy Discord app in this server first");
    }
    // Name comes from the user's snapshot, refreshed at each login.
    let name = state
        .db
        .user_guilds(session.discord_user_id)
        .ok()
        .and_then(|guilds| {
            guilds
                .into_iter()
                .find(|g| g.guild_id == guild_id)
                .map(|g| g.guild_name)
        })
        .unwrap_or_else(|| snowflake_string(guild_id));
    if let Err(e) = state
        .db
        .register_guild(guild_id, &name, session.discord_user_id)
    {
        return internal_error(e);
    }
    Json(serde_json::json!({ "registered": true })).into_response()
}

async fn api_list_telescopes(
    State(state): State<HubState>,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let guild_id = match parse_id_param(&guild_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    if let ManageAuth::Denied(response) = authorize_manage(&state, &headers, guild_id, false).await
    {
        return response;
    }
    match state.db.guild_telescopes(guild_id) {
        Ok(telescopes) => Json(serde_json::json!({
            "telescopes": telescopes
                .iter()
                .map(|t| {
                    let mut value = telescope_json(t);
                    value["connected"] =
                        serde_json::Value::from(state.rig_connections.get(t.id).is_some());
                    value
                })
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => internal_error(e),
    }
}

#[derive(Deserialize)]
struct CreateTelescopeBody {
    name: String,
}

async fn api_create_telescope(
    State(state): State<HubState>,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CreateTelescopeBody>,
) -> Response {
    let guild_id = match parse_id_param(&guild_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let session = match authorize_manage(&state, &headers, guild_id, true).await {
        ManageAuth::Ok(session) => session,
        ManageAuth::Denied(response) => return response,
    };
    let name = body.name.trim();
    if name.is_empty() || name.len() > 64 {
        return bad_request("telescope name must be 1-64 characters");
    }
    match state.db.get_guild(guild_id) {
        Ok(Some(_)) => {}
        Ok(None) => return bad_request("register this guild first"),
        Err(e) => return internal_error(e),
    }
    match state
        .db
        .create_telescope(guild_id, name, session.discord_user_id)
    {
        Ok(telescope) => Json(telescope_json(&telescope)).into_response(),
        Err(_) => bad_request("a telescope with this name already exists in this guild"),
    }
}

/// Load a telescope and authorize management of its guild.
async fn telescope_for_manage(
    state: &HubState,
    headers: &HeaderMap,
    telescope_id: &str,
    mutating: bool,
) -> Result<TelescopeRow, Response> {
    let id: i64 = telescope_id
        .parse()
        .map_err(|_| bad_request("invalid telescope id"))?;
    let telescope = match state.db.get_telescope(id) {
        Ok(Some(telescope)) => telescope,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "no such telescope").into_response()),
        Err(e) => return Err(internal_error(e)),
    };
    match authorize_manage(state, headers, telescope.guild_id, mutating).await {
        ManageAuth::Ok(_) => Ok(telescope),
        ManageAuth::Denied(response) => Err(response),
    }
}

#[derive(Deserialize, Default)]
struct UpdateTelescopeBody {
    /// Absent = keep; null = clear; string = set.
    #[serde(default, deserialize_with = "double_option")]
    discord_channel_id: Option<Option<String>>,
    image_cooldown_seconds: Option<i64>,
    write_policy: Option<String>,
    allowed_role_ids: Option<Vec<String>>,
}

/// Deserialize a field so "absent" and "null" stay distinguishable.
fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

async fn api_update_telescope(
    State(state): State<HubState>,
    Path(telescope_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateTelescopeBody>,
) -> Response {
    let telescope = match telescope_for_manage(&state, &headers, &telescope_id, true).await {
        Ok(telescope) => telescope,
        Err(response) => return response,
    };
    if let Some(policy) = &body.write_policy
        && policy != "disabled"
        && policy != "roles"
    {
        return bad_request("write_policy must be 'disabled' or 'roles'");
    }
    if let Some(cooldown) = body.image_cooldown_seconds
        && !(0..=86400).contains(&cooldown)
    {
        return bad_request("image_cooldown_seconds must be 0-86400");
    }
    let channel = match &body.discord_channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => match parse_snowflake(raw) {
            Ok(id) => Some(Some(id)),
            Err(_) => return bad_request("invalid discord_channel_id"),
        },
    };
    let roles = match &body.allowed_role_ids {
        None => None,
        Some(raw_roles) => {
            let mut parsed = Vec::new();
            for raw in raw_roles {
                match parse_snowflake(raw) {
                    Ok(id) => parsed.push(id),
                    Err(_) => return bad_request("invalid role id"),
                }
            }
            Some(parsed)
        }
    };
    let update = TelescopeUpdate {
        discord_channel_id: channel,
        image_cooldown_seconds: body.image_cooldown_seconds,
        write_policy: body.write_policy.clone(),
        allowed_role_ids: roles,
    };
    if let Err(e) = state.db.update_telescope(telescope.id, &update) {
        return internal_error(e);
    }
    match state.db.get_telescope(telescope.id) {
        Ok(Some(updated)) => Json(telescope_json(&updated)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such telescope").into_response(),
        Err(e) => internal_error(e),
    }
}

async fn api_delete_telescope(
    State(state): State<HubState>,
    Path(telescope_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let telescope = match telescope_for_manage(&state, &headers, &telescope_id, true).await {
        Ok(telescope) => telescope,
        Err(response) => return response,
    };
    match state.db.delete_telescope(telescope.id) {
        Ok(()) => Json(serde_json::json!({ "deleted": true })).into_response(),
        Err(e) => internal_error(e),
    }
}

/// Issue a fresh pairing token, revoking any live unconsumed ones so only
/// one token is outstanding per telescope.
async fn api_issue_pairing_token(
    State(state): State<HubState>,
    Path(telescope_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let telescope = match telescope_for_manage(&state, &headers, &telescope_id, true).await {
        Ok(telescope) => telescope,
        Err(response) => return response,
    };
    let session = match require_session_with_csrf(&state, &headers) {
        Some(session) => session,
        None => return (StatusCode::UNAUTHORIZED, "login required").into_response(),
    };
    if let Err(e) = state.db.revoke_pairing_tokens(telescope.id) {
        return internal_error(e);
    }
    match state
        .db
        .issue_pairing_token(telescope.id, session.discord_user_id)
    {
        Ok(token) => Json(serde_json::json!({
            "token": token,
            "expires_in_seconds": super::tenants::PAIRING_TOKEN_TTL_SECONDS,
        }))
        .into_response(),
        Err(e) => internal_error(e),
    }
}

async fn api_revoke_pairing_tokens(
    State(state): State<HubState>,
    Path(telescope_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let telescope = match telescope_for_manage(&state, &headers, &telescope_id, true).await {
        Ok(telescope) => telescope,
        Err(response) => return response,
    };
    match state.db.revoke_pairing_tokens(telescope.id) {
        Ok(revoked) => Json(serde_json::json!({ "revoked": revoked })).into_response(),
        Err(e) => internal_error(e),
    }
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
    let guild_checker: Option<Arc<dyn GuildChecker>> = if config.discord.bot_token.is_empty() {
        println!("No bot token configured; live guild checks are disabled");
        None
    } else {
        Some(Arc::new(CachedGuildChecker::new(
            SerenityGuildChecker::new(&config.discord.bot_token),
        )))
    };
    let rig_connections = Arc::new(super::direct_server::RigConnections::default());

    // With a bot token, run the central Discord bot and the per-rig chat
    // updater manager alongside the web server.
    if !config.discord.bot_token.is_empty() {
        let bot_config = crate::chat::DiscordBotConfig {
            enabled: true,
            token: config.discord.bot_token.clone(),
            application_id: None,
            public_key: None,
            default_channel_id: None,
            live_status: false,
            state_file: "chatstronomy-hub-state.json".to_string(),
            write_acl: Vec::new(),
        };
        let resolver = Arc::new(super::rig_resolver::HubRigResolver::new(
            db.clone(),
            rig_connections.clone(),
        ));
        let (service, _gateway) = crate::chat::run_bot(&bot_config, resolver).await?;
        let mut manager = crate::chat::ChatServiceManager::new();
        manager.add_service(Box::new(service));
        let updaters = Arc::new(super::updaters::UpdaterManager::new(
            db.clone(),
            rig_connections.clone(),
            Arc::new(manager),
        ));
        tokio::spawn(updaters.run());
        println!("Central Discord bot and chat updater manager started");
    }

    let state = HubState {
        db,
        config: Arc::new(config),
        oauth,
        guild_checker,
        rig_connections,
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

    async fn spawn_hub_with(
        config: HubConfig,
        checker: Option<Arc<dyn GuildChecker>>,
    ) -> (String, Db) {
        let db = Db::open_in_memory().unwrap();
        let oauth = if config.oauth_configured() {
            Some(Arc::new(
                DiscordOauthClient::new(
                    &config.discord.base_url,
                    &config.discord.client_id,
                    &config.discord.client_secret,
                    &config.public_base_url,
                )
                .unwrap(),
            ))
        } else {
            None
        };
        let state = HubState {
            db: db.clone(),
            config: Arc::new(config),
            oauth,
            guild_checker: checker,
            rig_connections: Arc::new(crate::hub::direct_server::RigConnections::default()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
        (format!("http://{addr}"), db)
    }

    async fn spawn_hub(config: HubConfig) -> (String, Db) {
        spawn_hub_with(config, None).await
    }

    /// Guild checker with fixed answers for tests.
    struct StubChecker {
        bot: bool,
        member: bool,
    }

    #[async_trait::async_trait]
    impl GuildChecker for StubChecker {
        async fn bot_in_guild(&self, _guild_id: u64) -> bool {
            self.bot
        }
        async fn user_in_guild(&self, _guild_id: u64, _user_id: u64) -> bool {
            self.member
        }
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

    // The stub Discord user owns guild 197038439483310086 ("Observatory")
    // and is a plain member of guild 300 ("Other").
    const OWNED_GUILD: &str = "197038439483310086";
    const MEMBER_GUILD: &str = "300";

    /// Log in through the stub Discord and return the CSRF token.
    async fn login(client: &reqwest::Client, base: &str) -> String {
        let state = login_state(client, base).await;
        client
            .get(format!(
                "{base}/oauth/callback?code=test-code&state={state}"
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = client
            .get(format!("{base}/api/session"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["csrf_token"].as_str().unwrap().to_string()
    }

    async fn managed_hub(
        checker: Option<Arc<dyn GuildChecker>>,
    ) -> (String, Db, reqwest::Client, String) {
        let stub = spawn_stub_discord().await;
        let (base, db) = spawn_hub_with(oauth_config(&stub), checker).await;
        let client = client();
        let csrf = login(&client, &base).await;
        (base, db, client, csrf)
    }

    #[tokio::test]
    async fn management_flow_register_create_pair() {
        let (base, db, client, csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: true,
            member: true,
        })))
        .await;

        // Guild list shows only the manageable guild, unregistered.
        let body: serde_json::Value = client
            .get(format!("{base}/api/guilds"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let guilds = body["guilds"].as_array().unwrap();
        assert_eq!(guilds.len(), 1);
        assert_eq!(guilds[0]["id"], OWNED_GUILD);
        assert_eq!(guilds[0]["registered"], false);
        assert_eq!(guilds[0]["bot_installed"], true);
        let install_url = guilds[0]["install_url"].as_str().unwrap();
        assert!(install_url.contains("client_id=client-1"));
        assert!(install_url.contains(&format!("guild_id={OWNED_GUILD}")));
        assert!(install_url.contains("scope=bot+applications.commands"));

        // Register, then create a telescope.
        let response = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/register"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let telescope: serde_json::Value = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/telescopes"))
            .header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "name": "c925" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(telescope["name"], "c925");
        assert_eq!(telescope["write_policy"], "disabled");
        let id = telescope["id"].as_i64().unwrap();

        // Update channel routing and write policy.
        let updated: serde_json::Value = client
            .patch(format!("{base}/api/telescopes/{id}"))
            .header("x-csrf-token", &csrf)
            .json(&serde_json::json!({
                "discord_channel_id": "555666777",
                "write_policy": "roles",
                "allowed_role_ids": ["1111", "2222"],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(updated["discord_channel_id"], "555666777");
        assert_eq!(updated["write_policy"], "roles");
        assert_eq!(updated["allowed_role_ids"][0], "1111");

        // Issue a pairing token; it must be consumable exactly once.
        let issued: serde_json::Value = client
            .post(format!("{base}/api/telescopes/{id}/pairing-token"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = issued["token"].as_str().unwrap();
        assert!(token.starts_with(super::super::tenants::PAIRING_TOKEN_PREFIX));
        assert_eq!(db.consume_pairing_token(token).unwrap(), Some(id));
        assert_eq!(db.consume_pairing_token(token).unwrap(), None);
    }

    #[tokio::test]
    async fn non_manager_cannot_register() {
        let (base, _db, client, csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: true,
            member: true,
        })))
        .await;
        let response = client
            .post(format!("{base}/api/guilds/{MEMBER_GUILD}/register"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn mutation_requires_csrf() {
        let (base, _db, client, _csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: true,
            member: true,
        })))
        .await;
        let response = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/register"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn departed_member_loses_access() {
        // Snapshot says owner, but the live check says they left.
        let (base, _db, client, csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: true,
            member: false,
        })))
        .await;
        let response = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/register"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn register_requires_bot_installed() {
        let (base, _db, client, csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: false,
            member: true,
        })))
        .await;
        let response = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/register"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn telescope_requires_registered_guild() {
        let (base, _db, client, csrf) = managed_hub(Some(Arc::new(StubChecker {
            bot: true,
            member: true,
        })))
        .await;
        let response = client
            .post(format!("{base}/api/guilds/{OWNED_GUILD}/telescopes"))
            .header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "name": "c925" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn guild_api_requires_login() {
        let stub = spawn_stub_discord().await;
        let (base, _db) = spawn_hub_with(
            oauth_config(&stub),
            Some(Arc::new(StubChecker {
                bot: true,
                member: true,
            })),
        )
        .await;
        let response = reqwest::get(format!("{base}/api/guilds")).await.unwrap();
        assert_eq!(response.status(), 401);
    }
}
