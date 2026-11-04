<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<svelte:head><title>Sign in · BBSBOX</title></svelte:head>

<h1>Sign in</h1>
<p>__BBSBOX_NODE_NAME__</p>
<p class="error" role="alert">__BBSBOX_LOGIN_MESSAGE__</p>
<form action="/login/" method="post">
  <label>Username <input name="username" autocomplete="username" maxlength="32" required></label>
  <label>Password <input name="password" type="password" autocomplete="current-password" maxlength="1024" required></label>
  <button type="submit">Sign in</button>
</form>
