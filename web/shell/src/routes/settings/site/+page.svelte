<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<svelte:head><title>Site settings · BBSBOX</title></svelte:head>

<h1>Site settings</h1>
<p><a href="/account/">Account</a></p>
<p role="status">__BBSBOX_SITE_STATUS__</p>
<form action="/settings/site/" method="post">
  <input type="hidden" name="revision" value="__BBSBOX_SITE_REVISION__">
  <input type="hidden" name="csrf" value="__BBSBOX_CSRF__">
  <label>Welcome message <textarea name="welcome" rows="5" maxlength="1024">__BBSBOX_MESSAGE__</textarea></label>
  <label>Welcome text art <textarea name="artwork" rows="6" maxlength="2048" spellcheck="false">__BBSBOX_ART__</textarea></label>
  <label>Menu order <select name="menu">__BBSBOX_MENU_OPTIONS__</select></label>
  <button type="submit">Save site settings</button>
</form>
