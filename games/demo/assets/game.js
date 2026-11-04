"use strict";
// SPDX-License-Identifier: AGPL-3.0-or-later
(() => {
    const stateText = document.querySelector("#state");
    const notice = document.querySelector("#notice");
    const explore = document.querySelector("#explore");
    const rest = document.querySelector("#rest");
    const retry = document.querySelector("#retry");
    const memberships = document.querySelector("#memberships");
    const groups = document.querySelector("#groups");
    const more = document.querySelector("#more-groups");
    let bearer;
    let current;
    let pending;
    let busy = false;
    let groupCursor = null;
    class RejectedMove extends Error {
    }
    async function request(path, input) {
        const headers = new Headers();
        if (bearer)
            headers.set("Authorization", "Bearer " + bearer);
        if (input)
            headers.set("Content-Type", "application/json");
        const response = await fetch(path, { method: input ? "POST" : "GET", headers, credentials: "omit", body: input ? JSON.stringify(input) : undefined });
        if (!response.ok) {
            if (response.status === 401)
                bearer = undefined;
            const body = await response.json().catch(() => null);
            const message = body?.message ?? (response.status === 401 ? "Open the game again from BBSBOX." : "The game could not respond.");
            throw response.status < 500 ? new RejectedMove(message) : new Error(message);
        }
        return response.json();
    }
    function render() {
        const ready = !!bearer && !!current && current.players === 2 && current.seat === current.turn && !current.finished && !busy && !pending;
        explore.disabled = !ready || current.stamina === 0;
        rest.disabled = !ready || current.stamina >= 3;
        retry.hidden = !pending || !bearer;
        retry.disabled = busy;
        if (!current)
            return;
        const turn = current.finished ? "Run complete." : current.players < 2 ? "Waiting for a second player." : current.turn === current.seat ? "Your turn." : "The other player's turn.";
        stateText.textContent = `Chamber ${current.chamber} of 3. Stamina ${current.stamina} of 3. ${turn}`;
    }
    function show(error) {
        notice.textContent = error instanceof Error ? error.message : "The game could not respond.";
    }
    async function move(action) {
        if (!bearer || busy)
            return;
        if (action) {
            const bytes = crypto.getRandomValues(new Uint8Array(16));
            pending = { command: Array.from(bytes, value => value.toString(16).padStart(2, "0")).join(""), action };
        }
        if (!pending)
            return;
        busy = true;
        render();
        try {
            current = await request("/game/v1/play", pending);
            pending = undefined;
            notice.textContent = "";
        }
        catch (error) {
            if (error instanceof RejectedMove)
                pending = undefined;
            show(error);
        }
        finally {
            busy = false;
            render();
        }
    }
    async function refresh() {
        if (!bearer || busy || pending)
            return;
        busy = true;
        render();
        try {
            current = await request("/game/v1/state");
        }
        catch (error) {
            show(error);
        }
        finally {
            busy = false;
            render();
        }
    }
    async function loadGroups() {
        more.disabled = true;
        try {
            const page = await request("/game/v1/groups" + (groupCursor === null ? "" : "?before=" + groupCursor));
            for (const membership of page.memberships) {
                const row = document.createElement("li");
                row.textContent = `${membership.name} · ${membership.kind} · ${membership.role}`;
                groups.append(row);
            }
            memberships.hidden = groups.children.length === 0;
            groupCursor = page.next;
            more.hidden = groupCursor === null;
        }
        catch {
            // The worker and turn controls remain available if the group read is busy.
        }
        finally {
            more.disabled = false;
        }
    }
    explore.addEventListener("click", () => void move("explore"));
    rest.addEventListener("click", () => void move("rest"));
    retry.addEventListener("click", () => void move());
    more.addEventListener("click", () => void loadGroups());
    const ticket = new URLSearchParams(location.hash.slice(1)).get("ticket");
    history.replaceState(null, "", location.pathname);
    if (!ticket) {
        notice.textContent = "Open the game from BBSBOX.";
        return;
    }
    void request("/game/v1/exchange", { ticket }).then(opened => {
        bearer = opened.session;
        current = opened.state;
        render();
        void loadGroups();
        window.setInterval(() => void refresh(), 1500);
    }).catch(show);
})();
