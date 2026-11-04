// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local clan and cohort directory, profiles and roster settings.
use super::{
    RequestOrigin, Sessions, check_origin, escape, form, form_access, html, password, template,
    token,
};
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use bbsbox_groups::{self as groups, Details, Error, Group, GroupId, Kind, Role};
use bbsbox_identity::LocalAccountId;
use bbsbox_storage::{Command, disk::Class, open_reader, writer::Priority};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/groups.html");
mod members;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/groups/", get(directory))
        .merge(members::routes())
        .route(
            "/groups/new/",
            get(new_group)
                .post(create)
                .layer(DefaultBodyLimit::max(32 * 1024)),
        )
        .route("/groups/{id}/", get(profile))
        .route(
            "/groups/{id}/edit/",
            get(edit_group)
                .post(edit)
                .layer(DefaultBodyLimit::max(32 * 1024)),
        )
}

fn page(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[
                ("GROUPS_TITLE", &escape(title)),
                ("GROUPS_CONTENT", content),
            ],
        ),
    )
}

fn login() -> Response {
    Redirect::to("/login/").into_response()
}

fn problem(state: &Sessions, status: StatusCode) -> Response {
    let title = match status {
        StatusCode::NOT_FOUND => "Group unavailable",
        StatusCode::FORBIDDEN => "Group management is restricted to its managers",
        StatusCode::CONFLICT => "Review the current group before saving changes",
        StatusCode::UNPROCESSABLE_ENTITY => "Check the group name and description",
        _ => "Groups unavailable",
    };
    page(
        state,
        status,
        title,
        "<p><a href=\"/groups/\">Group directory</a></p>",
    )
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Clan => "Clan",
        Kind::Cohort => "Cohort",
    }
}

#[derive(Deserialize)]
struct Directory {
    kind: Option<String>,
    before: Option<i64>,
}

async fn directory(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Directory>,
) -> Result<Response, StatusCode> {
    if form_access(&state, &headers).await.is_none() {
        return Ok(login());
    }
    let kind = query
        .kind
        .as_deref()
        .map(|kind| Kind::parse(kind).ok_or(StatusCode::BAD_REQUEST))
        .transpose()?;
    let home = state.signer.id();
    let rows = state
        .read(move |_, path| {
            Ok(groups::directory(
                &open_reader(path)?,
                home,
                kind,
                query.before,
            )?)
        })
        .await?;
    let mut content = String::from(
        "<p><a href=\"/groups/new/\">Create a group</a> · <a href=\"/groups/invitations/\">Your group invitations</a></p><p><a href=\"/groups/\">All groups</a> · <a href=\"?kind=clan\">Clans</a> · <a href=\"?kind=cohort\">Cohorts</a></p>",
    );
    if rows.is_empty() {
        content.push_str("<p>No groups to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(groups::PAGE_SIZE) {
            let _ = write!(
                content,
                "<li><a href=\"/groups/{}/\">{}</a> · {}</li>",
                row.id,
                escape(&row.name),
                kind_name(row.kind)
            );
        }
        content.push_str("</ul>");
        if let Some(last) = rows
            .get(groups::PAGE_SIZE - 1)
            .filter(|_| rows.len() > groups::PAGE_SIZE)
        {
            let filter = kind.map_or_else(String::new, |kind| format!("&amp;kind={}", kind.key()));
            let _ = write!(
                content,
                "<p><a href=\"?before={}{filter}\">More groups</a></p>",
                last.sequence
            );
        }
    }
    Ok(page(&state, StatusCode::OK, "Groups", &content))
}

#[derive(Deserialize)]
struct RosterPage {
    after: Option<String>,
}

async fn read(
    state: &Sessions,
    id: &str,
    viewer: LocalAccountId,
    after: Option<String>,
) -> Result<groups::View, StatusCode> {
    let id = GroupId::parse(id).ok_or(StatusCode::NOT_FOUND)?;
    let home = state.signer.id();
    state
        .read(move |_, path| {
            let reader = open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            Ok(groups::view(&tx, home, id, viewer, after.as_deref()))
        })
        .await?
        .map_err(status)
}

async fn profile(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<RosterPage>,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let view = match read(&state, &id, account.id, query.after).await {
        Ok(view) => view,
        Err(status) => return Ok(problem(&state, status)),
    };
    let group = view.group;
    let mut content = format!(
        "<p><a href=\"/groups/\">Group directory</a> · {}</p>",
        kind_name(group.kind)
    );
    content.push_str(&bbsbox_discussions::render_markdown(
        &group.details.description,
    ));
    if view.role.is_some_and(Role::can_manage) {
        let _ = write!(
            content,
            "<p><a href=\"/groups/{}/edit/\">Edit group</a></p>",
            group.id
        );
    }
    content.push_str("<h2>Roster</h2>");
    if let Some(roster) = view.roster {
        content.push_str(if group.details.public_roster {
            "<p>Visible to all signed-in accounts.</p>"
        } else {
            "<p>Visible to group members.</p>"
        });
        content.push_str("<ul>");
        for entry in roster.iter().take(groups::PAGE_SIZE) {
            let role = match entry.role {
                Role::Creator => "Creator",
                Role::Manager => "Manager",
                Role::Member => "Member",
            };
            let username = &entry.username;
            let _ = write!(
                content,
                "<li><a href=\"/directory/{username}/\">{username}</a> · {role}</li>"
            );
        }
        content.push_str("</ul>");
        if let Some(last) = roster
            .get(groups::PAGE_SIZE - 1)
            .filter(|_| roster.len() > groups::PAGE_SIZE)
        {
            let _ = write!(
                content,
                "<p><a href=\"?after={}\">More members</a></p>",
                last.username
            );
        }
    } else {
        content.push_str("<p>The roster is visible to group members.</p>");
    }
    members::render(&mut content, &csrf, &group, view.role, view.invitation);
    Ok(page(&state, StatusCode::OK, &group.details.name, &content))
}

fn editor(state: &Sessions, csrf: &str, group: Option<Group>) -> Response {
    let mut fields = String::new();
    let (path, title, name, description, public_roster) = if let Some(group) = group {
        let _ = write!(
            fields,
            "<input type=\"hidden\" name=\"revision\" value=\"{}\">",
            group.revision
        );
        (
            format!("/groups/{}/edit/", group.id),
            "Edit group",
            group.details.name,
            group.details.description,
            group.details.public_roster,
        )
    } else {
        fields.push_str("<label>Group type <select name=\"kind\"><option value=\"clan\">Clan</option><option value=\"cohort\">Cohort</option></select></label>");
        (
            "/groups/new/".into(),
            "Create group",
            String::new(),
            String::new(),
            false,
        )
    };
    let checked = if public_roster { " checked" } else { "" };
    let _ = write!(
        fields,
        "<label>Name <input name=\"name\" value=\"{}\" maxlength=\"120\" required></label><label>Description <textarea name=\"description\" maxlength=\"8192\" rows=\"6\">\n{}</textarea></label><label class=\"choice\"><input type=\"checkbox\" name=\"public_roster\" value=\"on\"{checked}> Show the roster to all signed-in accounts</label>",
        escape(&name),
        escape(&description)
    );
    let mut content = String::from("<p><a href=\"/groups/\">Group directory</a></p>");
    form(&mut content, &path, csrf, title, &fields);
    page(state, StatusCode::OK, title, &content)
}

async fn new_group(State(state): State<Sessions>, headers: HeaderMap) -> Response {
    match form_access(&state, &headers).await {
        Some((_, csrf)) => editor(&state, &csrf, None),
        None => login(),
    }
}

async fn edit_group(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return login();
    };
    match read(&state, &id, account.id, None).await {
        Ok(view) if view.role.is_some_and(Role::can_manage) => {
            editor(&state, &csrf, Some(view.group))
        }
        Ok(_) => problem(&state, StatusCode::FORBIDDEN),
        Err(status) => problem(&state, status),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    kind: Option<String>,
    revision: Option<i64>,
    name: String,
    description: String,
    public_roster: Option<String>,
}

async fn create(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    save(state, origin, headers, None, payload).await
}

async fn edit(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    let id = GroupId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    save(state, origin, headers, Some(id), payload).await
}

async fn save(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    id: Option<GroupId>,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let session = token(&headers)?;
    let Form(change) = payload.map_err(|error| error.status())?;
    if change.request.len() != 32 || !change.request.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let kind = match (id, change.revision, change.kind.as_deref()) {
        (None, None, Some(kind)) => Some(Kind::parse(kind).ok_or(StatusCode::BAD_REQUEST)?),
        (Some(_), Some(_), None) => None,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let details = Details {
        name: change.name.trim().into(),
        description: change.description.replace("\r\n", "\n"),
        public_roster: change.public_roster.as_deref() == Some("on"),
    };
    let fingerprint = serde_json::to_vec(&(
        id.map(|id| id.to_string()),
        kind.map(Kind::key),
        change.revision,
        &details.name,
        &details.description,
        details.public_roster,
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let home = state.signer.id();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("groups:{}", account.id),
                    id: &change.request,
                    request: &fingerprint,
                },
                Class::Content,
                64 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &change.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth.mutation_account(tx, &actor).map_err(Error::Identity)?;
                    if account.id != current.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(current.id)
                },
                |tx, actor| {
                    let id = match (id, change.revision, kind) {
                        (None, None, Some(kind)) => {
                            groups::create(tx, home, actor, kind, &details)?
                        }
                        (Some(id), Some(revision), None) => {
                            groups::edit(tx, home, actor, id, revision, &details)?;
                            id
                        }
                        _ => return Err(Error::InvalidInput),
                    };
                    Ok(id.to_string().into_bytes())
                },
            )
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    match result {
        Ok(receipt) => {
            let key = std::str::from_utf8(&receipt.bytes)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let id = GroupId::parse(key).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Redirect::to(&format!("/groups/{id}/")).into_response())
        }
        Err(error) => Ok(problem(&state, status(error))),
    }
}

fn status(error: Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        Error::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Forbidden => StatusCode::FORBIDDEN,
        Error::Missing => StatusCode::NOT_FOUND,
        Error::Conflict | Error::Storage(Storage::ReusedCommandId) => StatusCode::CONFLICT,
        Error::Identity(error) => super::super::failure(error),
        Error::Storage(
            Storage::QueueFull
            | Storage::WriterClosed
            | Storage::MaintenancePaused
            | Storage::DiskPressure { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
