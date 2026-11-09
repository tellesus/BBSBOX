// SPDX-License-Identifier: AGPL-3.0-or-later
const invoke = window.__TAURI__?.core?.invoke;
const status = document.querySelector('#status');
const form = document.querySelector('#identity');
const passphrase = document.querySelector('#passphrase');
const controls = document.querySelector('#controls');
const unlock = form.querySelector('button[type="submit"]');
const createButton = document.querySelector('#create');
let ready = false;
let busy = false;

function working(value) {
  busy = value;
  controls.disabled = value || !ready;
  form.setAttribute('aria-busy', String(value));
  status.classList.toggle('working', value);
}

function display(identity) {
  document.querySelector('#person').textContent = identity.person ?? '';
  form.hidden = Boolean(identity.person);
  status.textContent = identity.person
    ? (identity.requires_rejoin ? 'Identity recovered. Private groups require a fresh join.' : 'Identity unlocked.')
    : 'Create or unlock your local identity.';
}

async function open(create) {
  if (!ready || busy) return;
  if (!form.reportValidity()) return;
  const value = passphrase.value;
  passphrase.value = '';
  working(true);
  const button = create ? createButton : unlock;
  button.textContent = create ? 'Creating…' : 'Unlocking…';
  status.textContent = create ? 'Creating identity…' : 'Unlocking identity…';
  try {
    display(await invoke('open_identity', { passphrase: value, create }));
  } catch (error) {
    status.textContent = String(error);
  } finally {
    unlock.textContent = 'Unlock identity';
    createButton.textContent = 'Create identity';
    working(false);
    if (!form.hidden) passphrase.focus();
  }
}

form.addEventListener('submit', event => { event.preventDefault(); open(false); });
createButton.addEventListener('click', () => open(true));

async function check() {
  if (typeof invoke !== 'function') return;
  document.querySelector('#launch-help').hidden = true;
  status.textContent = 'Starting desktop client…';
  working(true);
  let identity = await invoke('probe_status');
  ready = true;
  display(identity);
  working(false);
  if (identity.checking) {
    form.hidden = true;
    identity = await invoke('prepare_check');
    display(identity);
    window.location.href = 'https://127.0.0.1:1/replaced-client';
    setTimeout(async () => {
      try {
        await invoke('finish_check', { person: identity.person, documentUrl: window.location.href });
      } catch (error) {
        status.textContent = String(error);
      }
    }, 300);
  }
}
check().catch(error => {
  ready = false;
  working(false);
  form.hidden = true;
  status.textContent = `Client could not start: ${String(error)}. Close and reopen the desktop app.`;
});
