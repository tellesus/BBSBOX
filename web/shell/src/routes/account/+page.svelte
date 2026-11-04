<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<svelte:head><title>Account · BBSBOX</title></svelte:head>

<h1>Account</h1>
<dl>
  <dt>Username</dt><dd>__BBSBOX_USERNAME__</dd>
  <dt>Role</dt><dd>__BBSBOX_ROLE__</dd>
</dl>
__BBSBOX_SYSOP_LINK__
<form action="/account/sign-out/" method="post">
  <input type="hidden" name="csrf" value="__BBSBOX_CSRF__">
  <button type="submit">Sign out</button>
</form>
