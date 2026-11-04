<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  let { children } = $props();
</script>

<a class="skip" href="#main">Skip to content</a>
<header>
  <a class="brand" href="/">BBSBOX</a>
  <nav aria-label="Main">
    <a href="/">Home</a>
    <a href="/account/">Account</a>
    <a href="/status/">Status</a>
  </nav>
</header>
<main id="main" tabindex="-1">
  {@render children()}
</main>
