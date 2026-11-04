# Vendored Browser Dependencies

`alpine.min.js` is the unmodified `package/dist/cdn.min.js` from
[alpinejs 3.17.5 on npm](https://www.npmjs.com/package/alpinejs/v/3.17.5),
retrieved on 2026-11-04. It is served locally; browsers do not load this script
from a third-party CDN.

- Upstream: <https://github.com/alpinejs/alpine/releases/tag/v3.17.5>
- Tarball: <https://registry.npmjs.org/alpinejs/-/alpinejs-3.17.5.tgz>
- npm integrity (verified before extraction):
  `sha512-XewMqRC6zjy8iaUPn86dxjvooAsNai3hUtQbdn0A5H2w7Jt10xHGZ1SczN5+Dvp3UKkuSEt9moy/UaUWOwKdnQ==`
- Bundle SHA-256:
  `b4036e9e9ac22fa6f6ae1ba292e015e1f3e9c85d0d79e73e4c8eb9bb139b522e`
- License: [MIT](ALPINE-LICENSE.md), from the same upstream tag.

The bundle also contains `@vue/reactivity 3.5.41` and `@vue/shared 3.5.41`;
their upstream MIT notices remain embedded in the file. Security checks must
include these exact bundled versions, not only a newly resolved npm lockfile.

When updating, verify npm integrity, preserve upstream license notices, update
the page asset version and Service Worker cache, and run frontend tests plus
the real-browser smoke tests documented in [development](../../docs/development.md).
