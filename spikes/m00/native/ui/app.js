// SPDX-License-Identifier: AGPL-3.0-or-later
const { invoke } = window.__TAURI__.core;
const status = document.querySelector('#status');

async function check() {
  const identity = await invoke('probe_status');
  document.querySelector('#person').textContent = identity.person;
  status.textContent = 'Bundled interface connected to native identity.';
  if (identity.checking) {
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
