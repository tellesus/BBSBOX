// SPDX-License-Identifier: AGPL-3.0-or-later
import adapter from '@sveltejs/adapter-static';

export default {
  kit: {
    adapter: adapter(),
    paths: { relative: false },
    prerender: { crawl: false, entries: ['/', '/login', '/account', '/settings/theme', '/settings/site', '/settings/accounts', '/unavailable', '/discussions', '/mail', '/directory', '/feeds', '/events', '/groups', '/files'] }
  }
};
