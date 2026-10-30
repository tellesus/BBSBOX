import adapter from '@sveltejs/adapter-static';
export default { kit: {
  adapter: adapter({ pages: 'build', assets: 'build' }),
  prerender: { crawl: false, entries: ['/'] },
  csp: { mode: 'hash', directives: { 'default-src': ['self'], 'script-src': ['self'], 'style-src': ['self', 'unsafe-inline'], 'img-src': ['self', 'data:'], 'connect-src': ['self'], 'object-src': ['none'], 'base-uri': ['none'] } }
} };
