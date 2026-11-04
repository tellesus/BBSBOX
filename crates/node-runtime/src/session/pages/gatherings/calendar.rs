// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local event agenda, attendance and account-specific reminders.
use super::{
    Browse, Failure, GatheringId, RequestOrigin, Sessions, check_origin, error_response, escape,
    event_id, event_key, event_list, events, form, form_access, login, open_reader, page, password,
    problem, token,
};
use axum::{
    Extension, Form, Router,
    extract::{Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_gatherings::attendance;
use bbsbox_identity::LocalAccountId;
use bbsbox_storage::{
    Command,
    clock::{Clock, SystemClock},
    disk::Class,
    writer::Priority,
};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/events/calendar/", get(calendar))
        .route("/events/{id}/attendance/{operation}/", post(save))
}

async fn calendar(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let after = match (query.time, query.id.as_deref()) {
        (None, None) => None,
        (Some(time), Some(id)) => {
            Some((time, GatheringId::parse(id).ok_or(StatusCode::BAD_REQUEST)?))
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let now = SystemClock::default().wall_millis();
    let (rows, due) = state
        .read(move |_, path| {
            let reader = open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            Ok((
                events::upcoming(&tx, now, after)?,
                attendance::due(&tx, account.id, now)?,
            ))
        })
        .await?;
    let mut content = String::from(
        "<p><a href=\"/events/\">All events</a> · <a href=\"/events/invitations/\">Your invitations</a></p>",
    );
    if !due.is_empty() {
        content.push_str("<h2>Reminders due</h2><ul>");
        for (id, title) in due {
            let _ = write!(
                content,
                "<li><a href=\"/events/{id}/\">{}</a></li>",
                escape(&title)
            );
        }
        content.push_str("</ul>");
    }
    content.push_str("<h2>Upcoming</h2>");
    event_list(&mut content, &rows, "No upcoming events.");
    Ok(page(&state, StatusCode::OK, "Calendar", &content))
}

pub(super) async fn render_attendance(
    state: &Sessions,
    account: LocalAccountId,
    id: GatheringId,
    cancelled: bool,
    csrf: &str,
    content: &mut String,
) -> Result<(), StatusCode> {
    let ([going, maybe, not_going], rsvp, reminder) = state
        .read(move |_, path| {
            let reader = open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            Ok((
                attendance::counts(&tx, id)?,
                attendance::own(&tx, id, account)?,
                attendance::reminder(&tx, account, id)?,
            ))
        })
        .await?;
    let _ = write!(
        content,
        "<p>Last recorded responses: {going} going, {maybe} maybe, {not_going} not going.</p>"
    );
    if cancelled {
        return Ok(());
    }
    let previous = rsvp
        .map(|rsvp| event_key(rsvp.revision))
        .unwrap_or_default();
    let mut fields = format!(
        "<input type=\"hidden\" name=\"previous\" value=\"{previous}\"><label>Your response <select name=\"value\" required><option value=\"\">Choose a response</option>"
    );
    for (key, label) in [
        ("going", "Going"),
        ("maybe", "Maybe"),
        ("not-going", "Not going"),
    ] {
        let selected = if rsvp.is_some_and(|rsvp| rsvp.choice.key() == key) {
            " selected"
        } else {
            ""
        };
        let _ = write!(fields, "<option value=\"{key}\"{selected}>{label}</option>");
    }
    fields.push_str("</select></label>");
    form(
        content,
        &format!("/events/{id}/attendance/rsvp/"),
        csrf,
        "Save response",
        &fields,
    );
    let mut fields = String::from("<label>Show in Calendar <select name=\"value\">");
    for (minutes, label) in [
        (0, "No reminder"),
        (15, "15 minutes before"),
        (60, "1 hour before"),
        (1440, "1 day before"),
    ] {
        let selected = if reminder.unwrap_or(0) == minutes {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            fields,
            "<option value=\"{minutes}\"{selected}>{label}</option>"
        );
    }
    fields.push_str("</select></label>");
    form(
        content,
        &format!("/events/{id}/attendance/reminder/"),
        csrf,
        "Save reminder",
        &fields,
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    previous: Option<String>,
    value: String,
}

enum Change {
    Respond(attendance::Response),
    Remind(Option<i64>),
}

async fn save(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path((id, operation)): Path<(String, String)>,
    headers: HeaderMap,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let session = token(&headers)?;
    let id = GatheringId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    let Form(input) = input.map_err(|error| error.status())?;
    if input.request.len() != 32 || !input.request.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let change = match (operation.as_str(), input.previous.as_deref()) {
        ("rsvp", Some(previous)) => Change::Respond(attendance::Response {
            gathering: id,
            choice: attendance::Choice::parse(&input.value).ok_or(StatusCode::BAD_REQUEST)?,
            previous: (!previous.is_empty())
                .then(|| event_id(previous))
                .transpose()?,
        }),
        ("reminder", None) => Change::Remind(match input.value.as_str() {
            "0" => None,
            "15" => Some(15),
            "60" => Some(60),
            "1440" => Some(1440),
            _ => return Err(StatusCode::BAD_REQUEST),
        }),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let fingerprint =
        serde_json::to_vec(&("attendance", operation, id, &input.value, &input.previous))
            .map_err(|_| StatusCode::BAD_REQUEST)?;
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
                64 * 1024,
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
                    match change {
                        Change::Respond(response) => {
                            gatherings.respond(
                                tx,
                                &signer,
                                actor,
                                response,
                                SystemClock::default().wall_millis(),
                            )?;
                        }
                        Change::Remind(minutes) => {
                            attendance::set_reminder(tx, actor, id, minutes)?
                        }
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
