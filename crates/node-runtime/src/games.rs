// SPDX-License-Identifier: AGPL-3.0-or-later
//! The development game origin and its scoped, worker-backed JSON API.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bbsbox_credentials::NodeId;
use bbsbox_game_host::{
    Component, Manifest,
    protocol::{Move, Operation, Reason, ResultBody, State as GameState},
    sessions::{Authorized, PlayerId, Registry, Secret},
    worker::Worker,
};
use bbsbox_identity::{Auth, LocalAccountId, session::Mutation};
use bbsbox_profiles::components::{Link, Provider};
use bbsbox_storage::clock::SystemClock;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;

const MANIFEST: &[u8] = include_bytes!("../../../games/demo/manifest.json");
const PAGE: &str = include_str!("../../../games/demo/assets/index.html");
const SCRIPT: &str = include_str!("../../../games/demo/assets/game.js");
const CSS: &str = include_str!("../../../games/demo/assets/game.css");
const GAME_ID: &str = "dungeon-demo";

pub(super) fn has_retained_state(database: &Path) -> std::io::Result<bool> {
    let parent = database
        .parent()
        .ok_or_else(|| std::io::Error::other("missing node data directory"))?;
    match std::fs::symlink_metadata(parent.join("games")) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn bundled_manifest() -> crate::Result<Manifest> {
    let manifest = Manifest::parse(MANIFEST).map_err(|_| "invalid bundled game manifest")?;
    if manifest.game_id() != GAME_ID {
        return Err("wrong bundled game ID".into());
    }
    Ok(manifest)
}

pub(super) fn bundled_worker(
    database: &Path,
    manifest: &Manifest,
    create: bool,
) -> crate::Result<(PathBuf, Worker)> {
    let filename = if cfg!(windows) {
        "bbsbox-demo-worker.exe"
    } else {
        "bbsbox-demo-worker"
    };
    let executable = std::env::current_exe()?.with_file_name(filename);
    let metadata = std::fs::symlink_metadata(&executable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("the bundled demo worker must be a regular executable".into());
    }
    let root = database
        .parent()
        .ok_or("missing node data directory")?
        .join("games");
    let directory = root.join(manifest.game_id());
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    for path in [&root, &directory] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_dir() => (),
            Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
                builder.create(path)?
            }
            Ok(_) => return Err("game state requires a private local directory".into()),
            Err(error) => return Err(error.into()),
        }
    }
    if !create {
        for entry in std::fs::read_dir(&root)? {
            if entry?.file_name() != manifest.game_id() {
                return Err("unregistered game state prevents a complete backup".into());
            }
        }
    }
    let state_path = directory.join("state.sqlite");
    match std::fs::symlink_metadata(&state_path) {
        Ok(metadata) if metadata.file_type().is_file() => (),
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => (),
        Ok(_) => return Err("game state must be a regular database".into()),
        Err(error) => return Err(error.into()),
    }
    let launch = if create {
        Worker::launch
    } else {
        Worker::launch_checkpoint
    };
    let worker = launch(&executable, &state_path, manifest, Duration::from_secs(3))?;
    Ok((directory, worker))
}

pub(crate) struct Service {
    pub origin: String,
    database: PathBuf,
    node: NodeId,
    manifest: Manifest,
    worker: Arc<Mutex<Worker>>,
    sessions: Mutex<Registry<Arc<LaunchAuthority>>>,
    work: Arc<Semaphore>,
}

struct LaunchAuthority {
    auth: Arc<Auth>,
    actor: Mutation,
}

type Scope = Authorized<Arc<LaunchAuthority>>;

fn check_authority(database: &Path, scope: &Scope) -> Result<(), StatusCode> {
    let authority = &scope.parent;
    let unavailable = |_| StatusCode::SERVICE_UNAVAILABLE;
    let reader = bbsbox_storage::open_reader(database).map_err(unavailable)?;
    let tx = reader
        .unchecked_transaction()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match authority.auth.mutation_account(&tx, &authority.actor) {
        Ok(current) if current.id == scope.account => Ok(()),
        Ok(_) | Err(bbsbox_identity::Error::InvalidSession) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

impl Service {
    pub fn start(origin: String, database: PathBuf, node: NodeId) -> crate::Result<Self> {
        let manifest = bundled_manifest()?;
        let (_, worker) = bundled_worker(&database, &manifest, true)?;
        Ok(Self {
            origin,
            database,
            node,
            manifest,
            worker: Arc::new(Mutex::new(worker)),
            sessions: Mutex::new(Registry::new(Arc::new(SystemClock::default()))),
            work: Arc::new(Semaphore::new(1)),
        })
    }

    pub fn ticket(
        &self,
        account: LocalAccountId,
        auth: Arc<Auth>,
        actor: Mutation,
    ) -> Result<Secret, StatusCode> {
        self.sessions
            .lock()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .launch(
                self.node,
                &self.manifest,
                account,
                Arc::new(LaunchAuthority { auth, actor }),
            )
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    }

    fn scope(&self, headers: &HeaderMap) -> Result<Scope, StatusCode> {
        let bearer = bearer(headers)?;
        self.sessions
            .lock()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .authorize(&self.manifest, bearer)
            .map_err(|_| StatusCode::UNAUTHORIZED)
    }

    async fn call(&self, scope: Scope, operation: Operation) -> Result<GameState, Problem> {
        let permit = self
            .work
            .clone()
            .try_acquire_owned()
            .map_err(|_| Problem::Unavailable)?;
        let worker = self.worker.clone();
        let database = self.database.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            check_authority(&database, &scope).map_err(Problem::Access)?;
            worker
                .lock()
                .map_err(|_| Problem::Unavailable)?
                .call(operation)
                .map_err(|_| Problem::Unavailable)
        })
        .await
        .map_err(|_| Problem::Unavailable)??;
        match result {
            ResultBody::State { state } => Ok(state),
            ResultBody::Refused { reason } => Err(Problem::Game(reason)),
            _ => Err(Problem::Unavailable),
        }
    }
}

fn bearer(headers: &HeaderMap) -> Result<&str, StatusCode> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    values
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|_| values.next().is_none())
        .ok_or(StatusCode::UNAUTHORIZED)
}

pub(crate) fn profile_provider(service: Arc<Service>) -> Provider<Vec<Link>> {
    Arc::new(move |tx, viewer| {
        let account = bbsbox_identity::account(tx, viewer.profile)
            .map_err(bbsbox_profiles::Error::Identity)?;
        if !account.active
            || !service
                .manifest
                .profile_components()
                .contains(&Component::CharacterSummary)
        {
            return Ok(vec![]);
        }
        let player = PlayerId::for_account(service.node, &service.manifest, account.id);
        let result = service
            .worker
            .lock()
            .ok()
            .and_then(|mut worker| worker.call(Operation::Read { player }).ok());
        let Some(ResultBody::State { state }) = result else {
            return Ok(vec![]);
        };
        Ok(Link::new(
            &format!("Dungeon demo · Explorer {}", state.seat + 1),
            "/games/",
        )
        .into_iter()
        .collect())
    })
}

pub(super) fn router(service: Arc<Service>) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { crate::asset(PAGE, "text/html; charset=utf-8") }),
        )
        .route(
            "/game.js",
            get(|| async { crate::asset(SCRIPT, "text/javascript; charset=utf-8") }),
        )
        .route(
            "/game.css",
            get(|| async { crate::asset(CSS, "text/css; charset=utf-8") }),
        )
        .route("/game/v1/exchange", post(exchange))
        .route("/game/v1/state", get(read))
        .route("/game/v1/groups", get(groups))
        .route("/game/v1/play", post(play))
        .layer(DefaultBodyLimit::max(2048))
        .with_state(service)
}

#[derive(Deserialize)]
struct GroupPage {
    before: Option<i64>,
}

#[derive(Serialize)]
struct GameMembership {
    group: String,
    name: String,
    kind: &'static str,
    role: &'static str,
}

#[derive(Serialize)]
struct Memberships {
    memberships: Vec<GameMembership>,
    next: Option<i64>,
}

async fn groups(
    State(service): State<Arc<Service>>,
    headers: HeaderMap,
    Query(page): Query<GroupPage>,
) -> Result<Json<Memberships>, StatusCode> {
    let scope = service.scope(&headers)?;
    let database = service.database.clone();
    let node = service.node;
    let permit = service
        .work
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        check_authority(&database, &scope)?;
        let reader =
            bbsbox_storage::open_reader(database).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        bbsbox_groups::membership::local_memberships(&reader, node, scope.account, page.before)
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)??;
    let next = rows
        .get(bbsbox_groups::PAGE_SIZE - 1)
        .filter(|_| rows.len() > bbsbox_groups::PAGE_SIZE)
        .map(|last| last.sequence);
    let memberships = rows
        .into_iter()
        .take(bbsbox_groups::PAGE_SIZE)
        .map(|row| GameMembership {
            group: row.group.to_string(),
            name: row.name,
            kind: row.kind.key(),
            role: row.role.key(),
        })
        .collect();
    Ok(Json(Memberships { memberships, next }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ticket {
    ticket: String,
}

#[derive(Serialize)]
struct Opened<'a> {
    session: &'a str,
    state: GameState,
}

fn require_origin(headers: &HeaderMap, service: &Service) -> Result<(), StatusCode> {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    if origins.next().and_then(|value| value.to_str().ok()) != Some(service.origin.as_str())
        || origins.next().is_some()
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

async fn exchange(
    State(service): State<Arc<Service>>,
    headers: HeaderMap,
    Json(input): Json<Ticket>,
) -> Result<Response, StatusCode> {
    require_origin(&headers, &service)?;
    let ticket = zeroize::Zeroizing::new(input.ticket);
    let (session, scope) = {
        let mut grants = service
            .sessions
            .lock()
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let session = grants
            .exchange(&service.manifest, &ticket)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let scope = grants
            .authorize(&service.manifest, session.expose())
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        (session, scope)
    };
    let player = scope.player;
    match service.call(scope, Operation::Join { player }).await {
        Ok(state) => Ok(Json(Opened {
            session: session.expose(),
            state,
        })
        .into_response()),
        Err(problem) => {
            if let Ok(mut grants) = service.sessions.lock() {
                let _ = grants.revoke(&service.manifest, session.expose());
            }
            Ok(problem.into_response())
        }
    }
}

async fn read(
    State(service): State<Arc<Service>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let scope = service.scope(&headers)?;
    let player = scope.player;
    Ok(
        match service.call(scope, Operation::Read { player }).await {
            Ok(state) => Json(state).into_response(),
            Err(problem) => problem.into_response(),
        },
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Play {
    command: String,
    action: Move,
}

async fn play(
    State(service): State<Arc<Service>>,
    headers: HeaderMap,
    Json(input): Json<Play>,
) -> Result<Response, StatusCode> {
    require_origin(&headers, &service)?;
    let scope = service.scope(&headers)?;
    let player = scope.player;
    if input.command.len() != 32
        || !input
            .command
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut command = [0; 16];
    hex::decode_to_slice(&input.command, &mut command).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(
        match service
            .call(
                scope,
                Operation::Act {
                    player,
                    command,
                    action: input.action,
                },
            )
            .await
        {
            Ok(state) => Json(state).into_response(),
            Err(problem) => problem.into_response(),
        },
    )
}

enum Problem {
    Unavailable,
    Access(StatusCode),
    Game(Reason),
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let (code, message) = match self {
            Self::Access(code) => return code.into_response(),
            Self::Unavailable | Self::Game(Reason::Unavailable) => {
                (StatusCode::SERVICE_UNAVAILABLE, "Game worker unavailable")
            }
            Self::Game(Reason::Full) => (StatusCode::CONFLICT, "Both game seats are occupied"),
            Self::Game(Reason::Waiting) => (StatusCode::CONFLICT, "Waiting for a second player"),
            Self::Game(Reason::OutOfTurn) => {
                (StatusCode::CONFLICT, "It is the other player's turn")
            }
            Self::Game(Reason::InvalidMove) => (StatusCode::CONFLICT, "That move is unavailable"),
            Self::Game(Reason::Finished) => (StatusCode::CONFLICT, "The run is complete"),
            Self::Game(Reason::ChangedRetry) => (
                StatusCode::CONFLICT,
                "The saved request used a different move",
            ),
            Self::Game(Reason::Paused) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Game paused for a checkpoint",
            ),
            Self::Game(Reason::NotJoined) => (StatusCode::FORBIDDEN, "Open the game from BBSBOX"),
            Self::Game(Reason::Unsupported) => {
                (StatusCode::BAD_REQUEST, "Game request unsupported")
            }
        };
        (code, Json(serde_json::json!({ "message": message }))).into_response()
    }
}
