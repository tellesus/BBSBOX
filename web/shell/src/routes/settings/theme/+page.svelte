<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<svelte:head><title>Theme · BBSBOX</title></svelte:head>

<h1>Theme</h1>
<p><a href="/account/">Account</a></p>
<p role="status">__BBSBOX_THEME_STATUS__</p>
<form action="/settings/theme/preview/" method="post">
  <input type="hidden" name="revision" value="__BBSBOX_THEME_REVISION__">
  <input type="hidden" name="csrf" value="__BBSBOX_CSRF__">
  <label>Background <input type="color" name="background" value="__BBSBOX_BACKGROUND__" required></label>
  <label>Text <input type="color" name="text" value="__BBSBOX_TEXT__" required></label>
  <label>Links and buttons <input type="color" name="accent" value="__BBSBOX_ACCENT__" required></label>
  <label>Focus and notices <input type="color" name="secondary" value="__BBSBOX_SECONDARY__" required></label>
  <div class="actions">
    <button type="submit">Preview</button>
    <button type="submit" formaction="/settings/theme/apply/">Apply</button>
    <button type="submit" formaction="/settings/theme/reset/">Restore default</button>
  </div>
</form>
