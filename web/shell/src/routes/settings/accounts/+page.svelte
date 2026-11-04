<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<svelte:head><title>Create account · BBSBOX</title></svelte:head>

<h1>Create account</h1>
<p><a href="/account/">Account</a></p>
<p class="error" role="status">__BBSBOX_ACCOUNT_STATUS__</p>
<form action="/settings/accounts/" method="post">
  <input type="hidden" name="csrf" value="__BBSBOX_CSRF__">
  <label>Username <input name="username" autocomplete="off" maxlength="32" required></label>
  <label>Initial password <input type="password" name="password" autocomplete="new-password" minlength="15" maxlength="1024" required></label>
  <label>Role <select name="role"><option value="member">Member</option><option value="sysop">SysOp</option></select></label>
  <button type="submit">Create account</button>
</form>
