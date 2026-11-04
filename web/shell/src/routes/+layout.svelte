<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<script lang="ts">
  let { children } = $props();
</script>

<a class="skip" href="#main">Skip to content</a>
<header>
  <a class="brand" href="/">BBSBOX</a>
  <nav aria-label="Main">
    __BBSBOX_NAV__
  </nav>
</header>
<main id="main" tabindex="-1">
  <p id="shell-state" role="status" hidden></p>
  {@render children()}
</main>
