// SPDX-License-Identifier: AGPL-3.0-or-later
const state = document.querySelector('#shell-state');

function status(message) {
  state.textContent = message;
  state.hidden = !message;
}

const disconnected = () => status('Node unreachable. Check your local Wi-Fi connection.');
let connectionCheck = 0;
window.addEventListener('offline', () => {
  connectionCheck++;
  disconnected();
});
window.addEventListener('online', async () => {
  const attempt = ++connectionCheck;
  status('Checking node…');
  let reachable = false;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 3000);
  try {
    const response = await fetch('/api/v1/node', { cache: 'no-store', signal: controller.signal });
    reachable = response.ok;
  } catch {
    // The device can be online while the local node is still unreachable.
  } finally {
    clearTimeout(timeout);
  }
  if (attempt === connectionCheck) {
    if (reachable) status('');
    else disconnected();
  }
});
window.addEventListener('pageshow', () => {
  if (navigator.onLine) status('');
  else disconnected();
});
document.addEventListener('submit', () => {
  connectionCheck++;
  status('Waiting for the node…');
});
