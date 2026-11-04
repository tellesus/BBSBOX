// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local public updates and a chronological timeline from explicitly followed accounts.
use super::{
    RequestOrigin, Sessions, check_origin, escape, event_id, event_key, form, form_access, html,
    password, template, token,
};
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_event_codec::EventId;
use bbsbox_identity::{Account, account_by_username, normalize_username};
use bbsbox_profiles::components::{Link, Provider};
use bbsbox_social::friends;
use bbsbox_social::{self as social, Error, Feed, Timing};
use bbsbox_storage::{
    Command,
    clock::{Clock, SystemClock},
    disk::Class,
    open_reader,
    writer::Priority,
};
use serde::Deserialize;
use std::{fmt::Write, sync::Arc};
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/feeds.html");
const COMPOSE: &str = "<p>Signed-in local accounts can read this update.</p><label>Update <textarea name=\"text\" maxlength=\"16384\" rows=\"5\" required></textarea></label>";

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route(
            "/feeds/",
            get(timeline)
                .post(publish)
                .layer(DefaultBodyLimit::max(56 * 1024)),
        )
        .route("/feeds/{username}/", get(author))
        .route("/feeds/{username}/follow/", post(follow))
        .route("/feeds/{username}/friend/", post(friend))
        .route("/friends/", get(friend_list))
}

pub(in crate::session) fn profile_provider() -> Provider<Option<Link>> {
    Arc::new(|tx, viewer| {
        let owner = bbsbox_identity::account(tx, viewer.profile)
            .map_err(bbsbox_profiles::Error::Identity)?;
        // Updates currently share the signed-in local audience. The route checks this again.
        Ok(Link::new(
            "Public updates",
            &format!("/feeds/{}/", owner.username),
        ))
    })
}

fn page(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[("FEED_TITLE", &escape(title)), ("FEED_CONTENT", content)],
        ),
    )
}

fn unavailable(state: &Sessions) -> Response {
    page(
        state,
        StatusCode::NOT_FOUND,
        "Updates unavailable",
        "<p><a href=\"/feeds/\">Your feed</a></p>",
    )
}

#[derive(Deserialize)]
struct Browse {
    username: Option<String>,
    time: Option<i64>,
    id: Option<String>,
    post: Option<String>,
}

fn cursor(query: &Browse) -> Result<Option<(i64, EventId)>, StatusCode> {
    match (query.time, query.id.as_deref()) {
        (None, None) => Ok(None),
        (Some(time), Some(id)) if query.post.is_none() => Ok(Some((time, event_id(id)?))),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn timeline(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    if let Some(username) = query.username {
        return Ok(match normalize_username(username.trim()) {
            Ok(username) => Redirect::to(&format!("/feeds/{username}/")).into_response(),
            Err(_) => unavailable(&state),
        });
    }
    let before = cursor(&query)?;
    let posts = state
        .read(move |_, path| {
            Ok(social::posts(
                &open_reader(path)?,
                Feed::Following(account.id),
                before,
            )?)
        })
        .await?;
    let mut content = format!(
        "<p><a href=\"/feeds/{}/\">Your public updates</a> · <a href=\"/friends/\">Friends</a></p><form action=\"/feeds/\" method=\"get\"><label>Username <input name=\"username\" maxlength=\"32\" required></label><button type=\"submit\">Find updates</button></form>",
        account.username
    );
    form(&mut content, "/feeds/", &csrf, "Post an update", COMPOSE);
    if posts.is_empty() {
        content.push_str("<p>No updates from accounts you follow.</p>");
    }
    render_posts(&mut content, &posts);
    Ok(page(&state, StatusCode::OK, "Your feed", &content))
}

async fn author(
    State(state): State<Sessions>,
    Path(username): Path<String>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    let Some((viewer, csrf)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    let before = cursor(&query)?;
    let focus = query.post.as_deref().map(event_id).transpose()?;
    let result = state
        .read(move |_, path| {
            let connection = open_reader(path)?;
            let tx = connection.unchecked_transaction()?;
            let target = match account_by_username(&tx, &username) {
                Ok(target) => target,
                Err(
                    bbsbox_identity::Error::AccountMissing
                    | bbsbox_identity::Error::InvalidUsername,
                ) => return Ok(None),
                Err(error) => return Err(error),
            };
            let following = social::is_following(&tx, viewer.id, target.id)?;
            let friendship = friends::relationship(&tx, viewer.id, target.id)?;
            let selection = match focus {
                Some(post) => Feed::Post(target.id, post),
                None => Feed::Author(target.id),
            };
            let posts = social::posts(&tx, selection, before)?;
            Ok(Some((target, following, friendship, posts)))
        })
        .await?;
    let Some((author, following, friendship, posts)) = result else {
        return Ok(unavailable(&state));
    };
    if focus.is_some() && posts.is_empty() {
        return Ok(unavailable(&state));
    }
    let username = &author.username;
    let mut content = format!(
        "<p><a href=\"/feeds/\">Your feed</a> · <a href=\"/directory/{username}/\">Profile</a></p>"
    );
    if viewer.id == author.id {
        form(&mut content, "/feeds/", &csrf, "Post an update", COMPOSE);
    } else if author.active || following {
        let (action, label) = if following {
            ("no", "Unfollow")
        } else {
            ("yes", "Follow")
        };
        form(
            &mut content,
            &format!("/feeds/{username}/follow/"),
            &csrf,
            label,
            &format!("<input type=\"hidden\" name=\"follow\" value=\"{action}\">"),
        );
    }
    if viewer.id != author.id {
        friend_forms(&mut content, &author, friendship, &csrf);
    }
    if focus.is_some() {
        let _ = write!(
            content,
            "<p><a href=\"/feeds/{username}/\">All updates</a></p>"
        );
    }
    if posts.is_empty() {
        content.push_str("<p>No public updates.</p>");
    }
    render_posts(&mut content, &posts);
    Ok(page(
        &state,
        StatusCode::OK,
        &format!("{username}'s updates"),
        &content,
    ))
}

fn friend_forms(
    content: &mut String,
    author: &Account,
    relationship: Option<friends::Relationship>,
    csrf: &str,
) {
    use friends::Status;
    let generation = relationship
        .as_ref()
        .map(|row| hex::encode(row.generation))
        .unwrap_or_default();
    let choices: &[(&str, &str)] = match relationship.map(|row| row.status) {
        Some(Status::Friends) => &[("remove", "Remove friend")],
        Some(Status::Outgoing) => &[("cancel", "Cancel friend request")],
        Some(Status::Incoming) if author.active => &[
            ("accept", "Accept friend request"),
            ("decline", "Decline friend request"),
        ],
        Some(Status::Incoming) => &[("decline", "Decline friend request")],
        None if author.active => &[("request", "Send friend request")],
        None => &[],
    };
    let path = format!("/feeds/{}/friend/", author.username);
    for (action, label) in choices {
        let fields = format!(
            "<input type=\"hidden\" name=\"friend\" value=\"{action}\"><input type=\"hidden\" name=\"generation\" value=\"{generation}\">"
        );
        form(content, &path, csrf, label, &fields);
    }
}

#[derive(Deserialize)]
struct FriendList {
    after: Option<String>,
}

async fn friend_list(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<FriendList>,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    let after = query
        .after
        .as_deref()
        .map(normalize_username)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let rows = state
        .read(move |_, path| {
            Ok(friends::list(
                &open_reader(path)?,
                account.id,
                after.as_deref(),
            )?)
        })
        .await?;
    let mut content = String::from(
        "<p><a href=\"/feeds/\">Your feed</a> · <a href=\"/directory/\">Directory</a></p>",
    );
    if rows.is_empty() {
        content.push_str("<p>No friendships or requests to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(50) {
            let label = match row.status {
                friends::Status::Incoming => "Incoming request",
                friends::Status::Outgoing => "Request sent",
                friends::Status::Friends => "Friends",
            };
            let _ = write!(
                content,
                "<li><a href=\"/feeds/{}/\">{}</a> · {label}</li>",
                row.username, row.username
            );
        }
        content.push_str("</ul>");
        if rows.len() > 50 {
            let _ = write!(
                content,
                "<p><a href=\"?after={}\">More connections</a></p>",
                rows[49].username
            );
        }
    }
    Ok(page(&state, StatusCode::OK, "Friends", &content))
}

fn render_posts(content: &mut String, posts: &[social::Post]) {
    for post in posts.iter().take(social::PAGE_SIZE) {
        let key = event_key(post.id);
        let author = &post.username;
        let _ = write!(
            content,
            "<article id=\"post-{key}\"><h2><a href=\"/feeds/{author}/\">{author}</a></h2><p><a href=\"/feeds/{author}/?post={key}#post-{key}\">Link to update</a>"
        );
        if let Some(date) = &post.utc_time {
            let display = date.trim_end_matches('Z').replace('T', " ");
            let _ = write!(content, " · <time datetime=\"{date}\">{display} UTC</time>");
        }
        match post.timing {
            Timing::ArrivedLater => content.push_str(" · Arrived later"),
            Timing::ClockUncertain => content.push_str(" · Clock uncertain"),
            Timing::Ordinary => (),
        }
        content.push_str("</p>");
        content.push_str(&bbsbox_discussions::render_markdown(&post.text));
        content.push_str("</article>");
    }
    if let Some(last) = posts
        .get(social::PAGE_SIZE - 1)
        .filter(|_| posts.len() > social::PAGE_SIZE)
    {
        let _ = write!(
            content,
            "<p><a href=\"?time={}&amp;id={}\">Older updates</a></p>",
            last.authored_at,
            event_key(last.id)
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    text: Option<String>,
    follow: Option<String>,
    friend: Option<String>,
    generation: Option<String>,
}

enum Action {
    Publish(String),
    Follow {
        username: String,
        selected: bool,
    },
    Friend {
        username: String,
        choice: friends::Action,
        generation: Option<[u8; 16]>,
    },
}

async fn publish(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Result<Response, StatusCode> {
    let Form(mut form) = form.map_err(|error| error.status())?;
    let text = form
        .text
        .take()
        .filter(|_| form.follow.is_none() && form.friend.is_none() && form.generation.is_none())
        .ok_or(StatusCode::BAD_REQUEST)?;
    submit(
        state,
        origin,
        headers,
        form,
        Action::Publish(text.replace("\r\n", "\n")),
    )
    .await
}

async fn follow(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(username): Path<String>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Result<Response, StatusCode> {
    let Form(form) = form.map_err(|error| error.status())?;
    let selected = match (
        form.follow.as_deref(),
        form.text.as_deref(),
        form.friend.as_deref(),
        form.generation.as_deref(),
    ) {
        (Some("yes"), None, None, None) => true,
        (Some("no"), None, None, None) => false,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let username = normalize_username(&username).map_err(|_| StatusCode::NOT_FOUND)?;
    submit(
        state,
        origin,
        headers,
        form,
        Action::Follow { username, selected },
    )
    .await
}

async fn friend(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(username): Path<String>,
    headers: HeaderMap,
    payload: Result<Form<Submission>, FormRejection>,
) -> Result<Response, StatusCode> {
    let Form(form) = payload.map_err(|error| error.status())?;
    if form.follow.is_some() || form.text.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let choice = form
        .friend
        .as_deref()
        .and_then(friends::Action::parse)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let generation = match form.generation.as_deref() {
        Some("") => None,
        Some(value) if value.len() == 32 => {
            let mut bytes = [0; 16];
            hex::decode_to_slice(value, &mut bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
            Some(bytes)
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let username = normalize_username(&username).map_err(|_| StatusCode::NOT_FOUND)?;
    submit(
        state,
        origin,
        headers,
        form,
        Action::Friend {
            username,
            choice,
            generation,
        },
    )
    .await
}

async fn submit(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    form: Submission,
    action: Action,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let session = token(&headers)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    if form.request.len() != 32 || !form.request.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (path, fingerprint, publishing) = match &action {
        Action::Publish(text) => (
            format!("/feeds/{}/", account.username),
            serde_json::to_vec(&("post", text)),
            true,
        ),
        Action::Follow { username, selected } => (
            format!("/feeds/{username}/"),
            serde_json::to_vec(&("follow", username, selected)),
            false,
        ),
        Action::Friend {
            username,
            generation,
            ..
        } => (
            format!("/feeds/{username}/"),
            serde_json::to_vec(&("friend", username, &form.friend, generation)),
            false,
        ),
    };
    let fingerprint = fingerprint.map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let signer = state.signer.clone();
    let social = state.social.clone();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("social:{}", account.id),
                    id: &form.request,
                    request: &fingerprint,
                },
                Class::Content,
                128 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &form.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let account_now = auth.mutation_account(tx, &actor).map_err(Error::Identity)?;
                    if account_now.id != account.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(account_now)
                },
                |tx, current: Account| match action {
                    Action::Publish(text) => {
                        let now = SystemClock::default().wall_millis();
                        let post =
                            event_key(social.post(tx, &signer, current.id, &text, now, now)?);
                        Ok(post.into_bytes())
                    }
                    Action::Follow { username, selected } => {
                        let target = account_by_username(tx, &username).map_err(Error::Identity)?;
                        social::set_following(tx, current.id, target.id, selected)?;
                        Ok(Vec::new())
                    }
                    Action::Friend {
                        username,
                        choice,
                        generation,
                    } => {
                        let target = account_by_username(tx, &username).map_err(Error::Identity)?;
                        friends::change(tx, current.id, target.id, choice, generation)?;
                        Ok(Vec::new())
                    }
                },
            )
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    match result {
        Ok(receipt) => {
            let destination = if publishing {
                let post = String::from_utf8(receipt.bytes)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                event_id(&post).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                format!("{path}?post={post}#post-{post}")
            } else {
                path
            };
            Ok(Redirect::to(&destination).into_response())
        }
        Err(error) => {
            let status = status(error);
            let title = match status {
                StatusCode::UNPROCESSABLE_ENTITY => "Check the update text",
                StatusCode::CONFLICT => "Open a fresh form",
                _ => "Update unavailable",
            };
            Ok(page(
                &state,
                status,
                title,
                "<p><a href=\"/feeds/\">Your feed</a></p>",
            ))
        }
    }
}

fn status(error: Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        Error::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Forbidden => StatusCode::FORBIDDEN,
        Error::Identity(bbsbox_identity::Error::AccountMissing) => StatusCode::NOT_FOUND,
        Error::Identity(error) => super::super::failure(error),
        Error::Conflict | Error::Storage(Storage::ReusedCommandId) => StatusCode::CONFLICT,
        Error::Storage(
            Storage::QueueFull
            | Storage::WriterClosed
            | Storage::MaintenancePaused
            | Storage::DiskPressure { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
