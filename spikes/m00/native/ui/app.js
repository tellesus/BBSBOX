// SPDX-License-Identifier: AGPL-3.0-or-later
const { invoke } = window.__TAURI__.core;
const status = document.querySelector('#status');
const form = document.querySelector('#identity');
const passphrase = document.querySelector('#passphrase');

function display(identity) {
  document.querySelector('#person').textContent = identity.person ?? '';
  form.hidden = Boolean(identity.person);
  status.textContent = identity.person
    ? (identity.requires_rejoin ? 'Identity recovered. Private groups require a fresh join.' : 'Identity unlocked.')
    : 'Create or unlock your local identity.';
}

async function open(create) {
  if (!form.reportValidity()) return;
  const value = passphrase.value;
  passphrase.value = '';
  for (const button of form.querySelectorAll('button')) button.disabled = true;
  status.textContent = create ? 'Creating identity…' : 'Unlocking identity…';
  try {
    display(await invoke('open_identity', { passphrase: value, create }));
  } catch (error) {
    status.textContent = String(error);
  } finally {
    for (const button of form.querySelectorAll('button')) button.disabled = false;
  }
}

form.addEventListener('submit', event => { event.preventDefault(); open(false); });
document.querySelector('#create').addEventListener('click', () => open(true));

async function check() {
  let identity = await invoke('probe_status');
  display(identity);
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
check().catch(error => { status.textContent = String(error); });
