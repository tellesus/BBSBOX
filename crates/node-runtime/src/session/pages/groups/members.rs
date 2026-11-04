// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local group invitations, consent and management forms.
use super::{
    Error, Group, GroupId, RequestOrigin, Role, Sessions, check_origin, escape, form, form_access,
    login, open_reader, page, password, problem, status, token,
};
use axum::{
    Extension, Form, Router,
    extract::{Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_groups::membership::{self, Join, Management};
use bbsbox_storage::{Command, disk::Class, writer::Priority};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/groups/invitations/", get(inbox))
        .route("/groups/{id}/membership/", post(apply))
}

#[derive(Deserialize)]
struct Browse {
    before: Option<i64>,
}

async fn inbox(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let home = state.signer.id();
    let rows = state
        .read(move |_, path| {
            Ok(membership::received(
                &open_reader(path)?,
                home,
                account.id,
                query.before,
            )?)
        })
        .await?;
    let mut content = String::from("<p><a href=\"/groups/\">Group directory</a></p>");
    if rows.is_empty() {
        content.push_str("<p>No group invitations to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(50) {
            let _ = write!(
                content,
                "<li><a href=\"/groups/{}/\">{}</a> · Invited by {}</li>",
                row.group,
                escape(&row.name),
                escape(&row.invited_by)
            );
        }
        content.push_str("</ul>");
        if rows.len() > 50 {
            let _ = write!(
                content,
                "<p><a href=\"?before={}\">More invitations</a></p>",
                rows[49].sequence
            );
        }
    }
    Ok(page(
        &state,
        StatusCode::OK,
        "Your group invitations",
        &content,
    ))
}

pub(super) fn render(
    content: &mut String,
    csrf: &str,
    group: &Group,
    role: Option<Role>,
    invitation: Option<[u8; 16]>,
) {
    let path = format!("/groups/{}/membership/", group.id);
    if let Some(invitation) = invitation {
        let notice = if group.details.public_roster {
            "If you join, any signed-in account can see your username on this roster."
        } else {
            "If you join, other group members can see your username on this roster."
        };
        let _ = write!(content, "<p>{notice}</p>");
        let fields = format!(
            "<input type=\"hidden\" name=\"generation\" value=\"{}\"><input type=\"hidden\" name=\"revision\" value=\"{}\">",
            hex::encode(invitation),
            group.revision
        );
        control(content, &path, csrf, "accept", "Accept invitation", &fields);
        control(
            content,
            &path,
            csrf,
            "decline",
            "Decline invitation",
            &fields,
        );
    }
    let Some(role) = role else {
        return;
    };
    let revision = format!(
        "<input type=\"hidden\" name=\"revision\" value=\"{}\">",
        group.membership_revision
    );
    if role.can_manage() {
        let target = "<label>Username <input name=\"target\" maxlength=\"32\" required></label>";
        control(content, &path, csrf, "invite", "Invite an account", target);
        let fields = format!("{revision}{target}");
        control(content, &path, csrf, "promote", "Add a manager", &fields);
        control(
            content,
            &path,
            csrf,
            "handover",
            "Hand over management",
            &fields,
        );
        let confirm = format!(
            "{revision}<label class=\"choice\"><input type=\"checkbox\" name=\"confirm\" value=\"on\" required> Close this group and its pending invitations</label>"
        );
        control(content, &path, csrf, "dissolve", "Dissolve group", &confirm);
    }
    control(content, &path, csrf, "leave", "Leave group", &revision);
}

fn control(content: &mut String, path: &str, csrf: &str, action: &str, label: &str, fields: &str) {
    form(
        content,
        path,
        csrf,
        label,
        &format!("<input type=\"hidden\" name=\"action\" value=\"{action}\">{fields}"),
    );
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    action: String,
    generation: Option<String>,
    target: Option<String>,
    revision: Option<i64>,
    confirm: Option<String>,
}

enum Action<'a> {
    Invite(&'a str),
    Join(Join),
    Manage(i64, Management<'a>),
}

impl Input {
    fn action(&self) -> Result<Action<'_>, StatusCode> {
        let invalid = StatusCode::BAD_REQUEST;
        match (
            self.action.as_str(),
            self.target.as_deref(),
            self.generation.as_deref(),
            self.revision,
            self.confirm.as_deref(),
        ) {
            ("invite", Some(target), None, None, None) => Ok(Action::Invite(target.trim())),
            (operation @ ("accept" | "decline"), None, Some(key), Some(revision), None)
                if key.len() == 32 =>
            {
                let mut generation = [0; 16];
                hex::decode_to_slice(key, &mut generation).map_err(|_| invalid)?;
                Ok(Action::Join(Join {
                    generation,
                    displayed_profile_revision: revision,
                    accept: operation == "accept",
                }))
            }
            ("promote", Some(target), None, Some(revision), None) => {
                Ok(Action::Manage(revision, Management::Promote(target.trim())))
            }
            ("handover", Some(target), None, Some(revision), None) => Ok(Action::Manage(
                revision,
                Management::Handover(target.trim()),
            )),
            ("leave", None, None, Some(revision), None) => {
                Ok(Action::Manage(revision, Management::Leave))
            }
            ("dissolve", None, None, Some(revision), Some("on")) => {
                Ok(Action::Manage(revision, Management::Dissolve))
            }
            _ => Err(invalid),
        }
    }
}

async fn apply(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    input: Result<Form<Input>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let session = token(&headers)?;
    let id = GroupId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    let Form(input) = input.map_err(|error| error.status())?;
    if input.request.len() != 32 || !input.request.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    input.action()?;
    let fingerprint = serde_json::to_vec(&(
        "membership",
        id.to_string(),
        &input.action,
        &input.target,
        &input.generation,
        input.revision,
        &input.confirm,
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let dissolve = input.action == "dissolve";
    let leaving = input.action == "leave";
    let auth = state.auth.clone();
    let home = state.signer.id();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("groups:{}", account.id),
                    id: &input.request,
                    request: &fingerprint,
                },
                Class::Content,
                64 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &input.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth.mutation_account(tx, &actor).map_err(Error::Identity)?;
                    if current.id != account.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(current.id)
                },
                |tx, actor| {
                    match input.action().map_err(|_| Error::InvalidInput)? {
                        Action::Invite(target) => membership::invite(tx, home, id, actor, target)?,
                        Action::Join(join) => membership::decide(tx, home, id, actor, join)?,
                        Action::Manage(revision, change) => {
                            membership::manage(tx, home, id, actor, revision, change)?
                        }
                    }
                    Ok(Vec::new())
                },
            )
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    Ok(match result {
        Ok(_) if dissolve => Redirect::to("/groups/").into_response(),
        Ok(_) => Redirect::to(&format!("/groups/{id}/")).into_response(),
        Err(Error::Conflict) if leaving => page(
            &state,
            StatusCode::CONFLICT,
            "Review group membership",
            "<p>The last manager must hand over management or dissolve the group before leaving.</p><p><a href=\"/groups/\">Group directory</a></p>",
        ),
        Err(error) => problem(&state, status(error)),
    })
}
