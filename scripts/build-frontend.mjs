#!/usr/bin/env node
// Assembles static/index.html from static/index.tmpl.html + static/partials/*.
// No dependencies. Usage: node scripts/build-frontend.mjs [--check]
//
// Template syntax: a line consisting solely of
//   <!-- @include partials/<name>.html -->
// is replaced verbatim by the referenced file's contents (path relative to static/).
// Output is byte-stable: running the build twice produces identical files.

import {readFileSync, writeFileSync} from 'node:fs';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const staticDir = join(root, 'static');
const templatePath = join(staticDir, 'index.tmpl.html');
const outputPath = join(staticDir, 'index.html');

const INCLUDE_RE = /^\s*<!--\s*@include\s+(\S+)\s*-->\s*$/;

function render(path, seen = new Set()) {
  if (seen.has(path)) throw new Error(`Circular include: ${path}`);
  seen.add(path);
  const src = readFileSync(path, 'utf8');
  const out = [];
  for (const line of src.split('\n')) {
    const m = line.match(INCLUDE_RE);
    if (m) {
      const includePath = join(staticDir, m[1]);
      const rendered = render(includePath, new Set(seen));
      // Preserve partial content verbatim (drop a single trailing newline so
      // concatenation matches the original line layout exactly).
      out.push(rendered.endsWith('\n') ? rendered.slice(0, -1) : rendered);
    } else {
      out.push(line);
    }
  }
  return out.join('\n');
}

const built = render(templatePath);
const check = process.argv.includes('--check');
if (check) {
  const current = readFileSync(outputPath, 'utf8');
  if (current !== built) {
    console.error('static/index.html is out of date; run: node scripts/build-frontend.mjs');
    process.exit(1);
  }
  console.log('static/index.html is up to date.');
} else {
  writeFileSync(outputPath, built);
  console.log(`Wrote ${outputPath} (${Buffer.byteLength(built)} bytes)`);
}
