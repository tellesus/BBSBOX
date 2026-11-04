// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public local gatherings and organizer controls.
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
use bbsbox_gatherings::{
    self as events, Change, Details, Gathering, GatheringId, Occurrence, Schedule, ScheduleError,
    invitations,
};
use bbsbox_profiles::components::{Link, Provider};
use bbsbox_storage::{
    Command,
    clock::{Clock, SystemClock},
    disk::Class,
    open_reader,
    writer::Priority,
};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, sync::Arc};
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/events.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/events/", get(index))
        .route("/events/invitations/", get(inbox))
        .route(
            "/events/new/",
            get(new_page)
                .post(create)
                .layer(DefaultBodyLimit::max(56 * 1024)),
        )
        .route("/events/{id}/", get(view))
        .route(
            "/events/{id}/edit/",
            get(edit_page)
                .post(revise)
                .layer(DefaultBodyLimit::max(56 * 1024)),
        )
        .route("/events/{id}/cancel/", post(cancel))
        .route("/events/{id}/invite/", post(invite))
}

pub(in crate::session) fn profile_provider() -> Provider<Vec<Link>> {
    Arc::new(|tx, viewer| {
        Ok(events::list(tx, Some(viewer.profile), None)?
            .into_iter()
            .filter_map(|item| {
                let label = if item.cancelled {
                    format!("{} (cancelled)", item.details.title)
                } else {
                    item.details.title
                };
                Link::new(&label, &format!("/events/{}/", item.id))
            })
            .collect())
    })
}

fn page(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[
                ("EVENTS_TITLE", &escape(title)),
                ("EVENTS_CONTENT", content),
            ],
        ),
    )
}

fn problem(state: &Sessions, status: StatusCode, reason: &str) -> Response {
    page(
        state,
        status,
        reason,
        "<p><a href=\"/events/\">Events</a></p>",
    )
}

fn login() -> Response {
    Redirect::to("/login/").into_response()
}

#[derive(Deserialize)]
struct Browse {
    time: Option<i64>,
    id: Option<String>,
}

async fn index(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    if form_access(&state, &headers).await.is_none() {
        return Ok(login());
    }
    let after = match (query.time, query.id.as_deref()) {
        (None, None) => None,
        (Some(time), Some(id)) => {
            Some((time, GatheringId::parse(id).ok_or(StatusCode::BAD_REQUEST)?))
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let rows = state
        .read(move |_, path| Ok(events::list(&open_reader(path)?, None, after)?))
        .await?;
    let mut content = String::from(
        "<p><a href=\"/events/new/\">Create an event</a> · <a href=\"/events/invitations/\">Your invitations</a></p>",
    );
    if rows.is_empty() {
        content.push_str("<p>No events to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(events::PAGE_SIZE) {
            let cancelled = if row.cancelled { " · Cancelled" } else { "" };
            let _ = write!(
                content,
                "<li><a href=\"/events/{}/\">{}</a> · {}{cancelled}</li>",
                row.id,
                escape(&row.details.title),
                when(&row.details.schedule)
            );
        }
        content.push_str("</ul>");
        if let Some(last) = rows
            .get(events::PAGE_SIZE - 1)
            .filter(|_| rows.len() > events::PAGE_SIZE)
        {
            let _ = write!(
                content,
                "<p><a href=\"?time={}&amp;id={}\">More events</a></p>",
                last.details.schedule.utc_millis(),
                last.id
            );
        }
    }
    Ok(page(&state, StatusCode::OK, "Events", &content))
}

async fn inbox(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let before = match (query.time, query.id.as_deref()) {
        (None, None) => None,
        (Some(time), Some(event)) => Some((time, event_id(event)?)),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let rows = state
        .read(move |_, path| {
            Ok(invitations::received(
                &open_reader(path)?,
                account.id,
                before,
            )?)
        })
        .await?;
    let mut content = String::from("<p><a href=\"/events/\">Events</a></p>");
    if rows.is_empty() {
        content.push_str("<p>No invitations to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(invitations::PAGE_SIZE) {
            let cancelled = if row.cancelled { " · Cancelled" } else { "" };
            let _ = write!(
                content,
                "<li><a href=\"/events/{}/\">{}</a> · Invited by {}{cancelled}</li>",
                row.gathering,
                escape(&row.title),
                escape(&row.organizer)
            );
        }
        content.push_str("</ul>");
        if let Some(last) = rows
            .get(invitations::PAGE_SIZE - 1)
            .filter(|_| rows.len() > invitations::PAGE_SIZE)
        {
            let _ = write!(
                content,
                "<p><a href=\"?time={}&amp;id={}\">Older invitations</a></p>",
                last.sent_at,
                event_key(last.event)
            );
        }
    }
    Ok(page(&state, StatusCode::OK, "Your invitations", &content))
}

fn when(schedule: &Schedule) -> String {
    let local = schedule.local().replace('T', " ");
    let seconds = schedule.offset_seconds().unsigned_abs();
    let sign = if schedule.offset_seconds() < 0 {
        '-'
    } else {
        '+'
    };
    let mut offset = format!("{sign}{:02}:{:02}", seconds / 3600, (seconds / 60) % 60);
    if !seconds.is_multiple_of(60) {
        let _ = write!(offset, ":{:02}", seconds % 60);
    }
    format!(
        "{} · {} (UTC{offset})",
        escape(&local),
        escape(schedule.zone())
    )
}

async fn read_one(state: &Sessions, id: &str) -> Result<Option<Gathering>, StatusCode> {
    let Some(id) = GatheringId::parse(id) else {
        return Ok(None);
    };
    state
        .read(move |_, path| Ok(events::get(&open_reader(path)?, id)?))
        .await
}

async fn view(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let Some(item) = read_one(&state, &id).await? else {
        return Ok(problem(&state, StatusCode::NOT_FOUND, "Event unavailable"));
    };
    let id = item.id;
    let details = item.details;
    let mut content = String::from("<p><a href=\"/events/\">Events</a></p>");
    if item.cancelled {
        content.push_str("<p><strong>Cancelled</strong></p>");
    }
    let _ = write!(
        content,
        "<dl><dt>When</dt><dd>{}</dd><dt>Where</dt><dd>{}</dd><dt>Host</dt><dd><a href=\"/directory/{}/\">{}</a></dd></dl>",
        when(&details.schedule),
        escape(&details.place),
        item.username,
        item.username
    );
    content.push_str(&bbsbox_discussions::render_markdown(&details.description));
    if account.id.bytes() == &item.organizer {
        let count = state
            .read(move |_, path| {
                Ok(invitations::sent_count(
                    &open_reader(path)?,
                    account.id,
                    id,
                )?)
            })
            .await?;
        let _ = write!(content, "<p>Invitations sent on this node: {count}</p>");
    }
    if account.id.bytes() == &item.organizer && !item.cancelled {
        let _ = write!(
            content,
            "<p><a href=\"/events/{id}/edit/\">Edit event</a></p>"
        );
        let fields = format!(
            "<input type=\"hidden\" name=\"previous\" value=\"{}\">",
            event_key(item.revision)
        );
        form(
            &mut content,
            &format!("/events/{id}/cancel/"),
            &csrf,
            "Cancel event",
            &fields,
        );
        form(
            &mut content,
            &format!("/events/{id}/invite/"),
            &csrf,
            "Send invitations",
            "<label>Local usernames, separated by commas <input name=\"recipients\" maxlength=\"660\" required></label>",
        );
    }
    Ok(page(&state, StatusCode::OK, &details.title, &content))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InviteInput {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    recipients: String,
}

async fn invite(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    input: Result<Form<InviteInput>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let session = token(&headers)?;
    let id = GatheringId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    let Form(input) = input.map_err(|error| error.status())?;
    if input.request.len() != 32
        || !input.request.bytes().all(|b| b.is_ascii_hexdigit())
        || input.recipients.len() > 660
        || input.recipients.split(',').count() > 20
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut recipients = match input
        .recipients
        .split(',')
        .map(|name| bbsbox_identity::normalize_username(name.trim()))
        .collect::<bbsbox_identity::Result<Vec<_>>>()
    {
        Ok(names) => names,
        Err(_) => {
            return Ok(problem(
                &state,
                StatusCode::UNPROCESSABLE_ENTITY,
                "Check the local usernames",
            ));
        }
    };
    recipients.sort_unstable();
    recipients.dedup();
    let fingerprint =
        serde_json::to_vec(&("invite", id, &recipients)).map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let signer = state.signer.clone();
    let gatherings = state.gatherings.clone();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("gatherings:{}", account.id),
                    id: &input.request,
                    request: &fingerprint,
                },
                Class::Content,
                128 * 1024,
                |tx| -> Result<_, Failure> {
                    let actor = auth
                        .authorize_mutation(tx, &session, &input.csrf, &origin.0)
                        .map_err(events::Error::Identity)?;
                    let current = auth
                        .mutation_account(tx, &actor)
                        .map_err(events::Error::Identity)?;
                    if current.id != account.id {
                        return Err(events::Error::Forbidden.into());
                    }
                    Ok(current.id)
                },
                |tx, actor| {
                    let now = SystemClock::default().wall_millis();
                    for username in recipients {
                        gatherings.invite(tx, &signer, actor, id, &username, now)?;
                    }
                    Ok(Vec::new())
                },
            )
        })
        .map_err(|error| error_response(error.into()).0)?
        .wait()
        .await;
    match result {
        Ok(_) => Ok(Redirect::to(&format!("/events/{id}/")).into_response()),
        Err(error) => {
            let (status, reason) = error_response(error);
            Ok(problem(&state, status, reason))
        }
    }
}

fn editor(state: &Sessions, csrf: &str, item: Option<Gathering>) -> Response {
    let (path, label) = item.as_ref().map_or_else(
        || ("/events/new/".into(), "Create event"),
        |item| (format!("/events/{}/edit/", item.id), "Save changes"),
    );
    let mut fields = String::new();
    if let Some(item) = &item {
        let _ = write!(
            fields,
            "<input type=\"hidden\" name=\"previous\" value=\"{}\">",
            event_key(item.revision)
        );
    }
    let detail = item.as_ref().map(|item| &item.details);
    let title = detail.map_or("", |item| &item.title);
    let place = detail.map_or("", |item| &item.place);
    let description = detail.map_or("", |item| &item.description);
    let date = detail.map_or("", |item| item.schedule.local());
    let zone = detail.map_or("UTC", |item| item.schedule.zone());
    let _ = write!(
        fields,
        "<label>Title <input name=\"title\" maxlength=\"160\" value=\"{}\" required></label><label>Place <input name=\"place\" maxlength=\"400\" value=\"{}\" required></label><label>Local date and time <input type=\"datetime-local\" name=\"local\" value=\"{}\" required></label><label>Time zone <input name=\"zone\" maxlength=\"96\" value=\"{}\" required></label><p>Use a time zone name such as America/Chicago.</p><label>If the time occurs twice <select name=\"occurrence\"><option value=\"\">Choose if needed</option><option value=\"first\">First occurrence</option><option value=\"second\">Second occurrence</option></select></label><label>Description <textarea name=\"description\" maxlength=\"16384\" rows=\"6\">\n{}</textarea></label>",
        escape(title),
        escape(place),
        escape(date),
        escape(zone),
        escape(description)
    );
    let mut content = String::from("<p><a href=\"/events/\">Events</a></p>");
    form(&mut content, &path, csrf, label, &fields);
    page(
        state,
        StatusCode::OK,
        if item.is_some() {
            "Edit event"
        } else {
            "New event"
        },
        &content,
    )
}

async fn new_page(State(state): State<Sessions>, headers: HeaderMap) -> Response {
    match form_access(&state, &headers).await {
        Some((_, csrf)) => editor(&state, &csrf, None),
        None => login(),
    }
}

async fn edit_page(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let Some(item) = read_one(&state, &id).await? else {
        return Ok(problem(&state, StatusCode::NOT_FOUND, "Event unavailable"));
    };
    if account.id.bytes() != &item.organizer {
        return Ok(problem(
            &state,
            StatusCode::FORBIDDEN,
            "Only the host can edit this event",
        ));
    }
    if item.cancelled {
        return Ok(problem(
            &state,
            StatusCode::CONFLICT,
            "This event was cancelled",
        ));
    }
    Ok(editor(&state, &csrf, Some(item)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    previous: Option<String>,
    title: Option<String>,
    place: Option<String>,
    local: Option<String>,
    zone: Option<String>,
    occurrence: Option<String>,
    description: Option<String>,
}

#[derive(Clone, Copy)]
enum Target {
    New,
    Edit(GatheringId),
    Cancel(GatheringId),
}

#[derive(Serialize)]
struct Draft {
    title: String,
    place: String,
    local: String,
    zone: String,
    occurrence: String,
    description: String,
}

impl Draft {
    fn details(self, previous: Option<&Schedule>) -> Result<Details, Failure> {
        let occurrence = match self.occurrence.as_str() {
            "" => None,
            "first" => Some(Occurrence::First),
            "second" => Some(Occurrence::Second),
            _ => return Err(Failure::Schedule(ScheduleError::InvalidDate)),
        };
        let schedule = match previous.filter(|item| {
            occurrence.is_none() && item.local() == self.local && item.zone() == self.zone
        }) {
            Some(previous) => previous.clone(),
            None => {
                Schedule::resolve(&self.local, &self.zone, occurrence).map_err(Failure::Schedule)?
            }
        };
        Ok(Details {
            title: self.title,
            place: self.place,
            description: self.description,
            schedule,
        })
    }
}

#[derive(Debug)]
enum Failure {
    Event(events::Error),
    Schedule(ScheduleError),
}

impl From<events::Error> for Failure {
    fn from(error: events::Error) -> Self {
        Self::Event(error)
    }
}
impl From<bbsbox_storage::Error> for Failure {
    fn from(error: bbsbox_storage::Error) -> Self {
        Self::Event(error.into())
    }
}

async fn create(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    submit(state, origin, headers, Target::New, input).await
}

async fn revise(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    let id = GatheringId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    submit(state, origin, headers, Target::Edit(id), input).await
}

async fn cancel(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    let id = GatheringId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    submit(state, origin, headers, Target::Cancel(id), input).await
}

async fn submit(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    target: Target,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let session = token(&headers)?;
    let Form(input) = input.map_err(|error| error.status())?;
    if input.request.len() != 32 || !input.request.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let previous = input.previous.as_deref().map(event_id).transpose()?;
    let (kind, id) = match (target, previous) {
        (Target::New, None) => ("new", None),
        (Target::Edit(id), Some(_)) => ("edit", Some(id)),
        (Target::Cancel(id), Some(_)) => ("cancel", Some(id)),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let draft = match (
        input.title,
        input.place,
        input.local,
        input.zone,
        input.occurrence,
        input.description,
    ) {
        (
            Some(title),
            Some(place),
            Some(local),
            Some(zone),
            Some(occurrence),
            Some(description),
        ) if kind != "cancel" => Some(Draft {
            title,
            place,
            local,
            zone,
            occurrence,
            description: description.replace("\r\n", "\n"),
        }),
        (None, None, None, None, None, None) if kind == "cancel" => None,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let fingerprint = serde_json::to_vec(&(kind, id, previous.map(event_key), &draft))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let signer = state.signer.clone();
    let gatherings = state.gatherings.clone();
    let receipt = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("gatherings:{}", account.id),
                    id: &input.request,
                    request: &fingerprint,
                },
                Class::Content,
                128 * 1024,
                |tx| -> Result<_, Failure> {
                    let actor = auth
                        .authorize_mutation(tx, &session, &input.csrf, &origin.0)
                        .map_err(events::Error::Identity)?;
                    let current = auth
                        .mutation_account(tx, &actor)
                        .map_err(events::Error::Identity)?;
                    if current.id != account.id {
                        return Err(events::Error::Forbidden.into());
                    }
                    Ok(current.id)
                },
                |tx, actor| {
                    let action = match (id, previous, draft) {
                        (None, None, Some(draft)) => Change::Create(draft.details(None)?),
                        (Some(id), Some(previous), Some(draft)) => {
                            let current = events::get(tx, id)
                                .map_err(events::Error::from)?
                                .ok_or(events::Error::Missing)?;
                            if actor.bytes() != &current.organizer {
                                return Err(events::Error::Forbidden.into());
                            }
                            if current.revision != previous || current.cancelled {
                                return Err(events::Error::Conflict.into());
                            }
                            Change::Revise {
                                id,
                                previous,
                                details: draft.details(Some(&current.details.schedule))?,
                            }
                        }
                        (Some(id), Some(previous), None) => Change::Cancel { id, previous },
                        _ => return Err(events::Error::InvalidInput.into()),
                    };
                    let (id, _) = gatherings.record(
                        tx,
                        &signer,
                        actor,
                        action,
                        SystemClock::default().wall_millis(),
                    )?;
                    Ok(id.to_string().into_bytes())
                },
            )
        })
        .map_err(|error| error_response(error.into()).0)?
        .wait()
        .await;
    match receipt {
        Ok(receipt) => {
            let key = std::str::from_utf8(&receipt.bytes)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let id = GatheringId::parse(key).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Redirect::to(&format!("/events/{id}/")).into_response())
        }
        Err(error) => {
            let (status, reason) = error_response(error);
            Ok(problem(&state, status, reason))
        }
    }
}

fn error_response(error: Failure) -> (StatusCode, &'static str) {
    use bbsbox_storage::Error as Storage;
    match error {
        Failure::Schedule(ScheduleError::SkippedTime) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "This local time is skipped. Choose another time",
        ),
        Failure::Schedule(ScheduleError::RepeatedTime) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "This local time occurs twice. Choose the first or second occurrence",
        ),
        Failure::Schedule(ScheduleError::UnknownZone) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Enter a valid time zone name",
        ),
        Failure::Schedule(ScheduleError::InvalidDate)
        | Failure::Event(events::Error::InvalidInput) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Check the date and event details",
        ),
        Failure::Event(
            events::Error::Conflict | events::Error::Storage(Storage::ReusedCommandId),
        ) => (
            StatusCode::CONFLICT,
            "Review the latest event before changing it",
        ),
        Failure::Event(events::Error::Forbidden) => {
            (StatusCode::FORBIDDEN, "Only the host can change this event")
        }
        Failure::Event(events::Error::Missing) => (StatusCode::NOT_FOUND, "Event unavailable"),
        Failure::Event(events::Error::Identity(bbsbox_identity::Error::AccountMissing)) => {
            (StatusCode::NOT_FOUND, "A recipient account is unavailable")
        }
        Failure::Event(events::Error::Identity(error)) => {
            (super::super::failure(error), "Event change refused")
        }
        Failure::Event(events::Error::Storage(
            Storage::QueueFull
            | Storage::WriterClosed
            | Storage::MaintenancePaused
            | Storage::DiskPressure { .. },
        )) => (StatusCode::SERVICE_UNAVAILABLE, "Try again shortly"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Event unavailable"),
    }
}
