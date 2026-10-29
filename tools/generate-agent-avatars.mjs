// Regenerates the robot pictures bundled with the web client for agents.
// The images are committed; run this only to change them:
//   npm install --prefix <dir> @dicebear/core@9.4.2 @dicebear/collection@9.4.2
//   node tools/generate-agent-avatars.mjs <dir>
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const modules = resolve(process.argv[2] ?? '.', 'node_modules');
const load = name => import(pathToFileURL(join(modules, name, 'lib/index.js')).href);
const { createAvatar } = await load('@dicebear/core');
const { botttsNeutral } = await load('@dicebear/collection');
const seeds = ['Atlas', 'Nova', 'Pixel', 'Juniper', 'Orbit', 'Maple', 'Sage', 'Echo', 'Comet', 'Willow', 'Ember', 'River',
  'Cedar', 'Quill', 'Harbor', 'Indigo', 'Linden', 'Moss', 'Pebble', 'Saffron', 'Tide', 'Wren', 'Zephyr', 'Birch'];
const out = resolve('web/public/agent-avatars');
mkdirSync(out, { recursive: true });
seeds.forEach((seed, index) => {
  // Square artwork; the client rounds every avatar, people and agents alike.
  const svg = createAvatar(botttsNeutral, { seed, size: 96 }).toString().replace(/<metadata[\s\S]*?<\/metadata>/, '');
  writeFileSync(join(out, `robot-${String(index + 1).padStart(2, '0')}.svg`), `${svg}\n`);
});
console.log(`Wrote ${seeds.length} pictures to ${out}`);
