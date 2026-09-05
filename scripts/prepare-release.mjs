import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

// Run only in the clean CI checkout after all checks have passed.
const read = (path) => readFileSync(path, 'utf8');
const manifest = read('Cargo.toml');
const current = manifest.match(/^version = "(\d+\.\d+\.\d+)"/m)?.[1];
if (!current) throw new Error('Workspace version is not a stable semver');
const parts = current.split('.').map(Number);
parts[2] += 1;
const next = parts.join('.');
const tag = `v${next}`;
const tags = execFileSync('git', ['tag', '--list', tag], { encoding: 'utf8' }).trim();
if (tags) throw new Error(`${tag} already exists; refusing to overwrite a release`);

const updates = new Map();
updates.set('Cargo.toml', manifest.replace(`version = "${current}"`, `version = "${next}"`));
const dependencyPath = 'crates/omnyssh/Cargo.toml';
updates.set(dependencyPath, read(dependencyPath).replace(`version = "${current}"`, `version = "${next}"`));
let lockCount = 0;
updates.set('Cargo.lock', read('Cargo.lock').replace(
  /(name = "omnyssh(?:-core|-gui)?"\r?\nversion = ")[^"]+"/g,
  (match, prefix) => { lockCount += 1; return `${prefix}${next}"`; }
));
if (lockCount !== 3) throw new Error('Expected three workspace packages in Cargo.lock');
for (const path of ['README.md', 'README.en.md', 'doc/omny.1', 'doc/zh_CN/omny.1']) {
  updates.set(path, read(path).replaceAll(current, next));
}
const changelog = read('CHANGELOG.md');
const unreleased = changelog.match(/## Unreleased\r?\n([\s\S]*?)(?=\r?\n## )/);
if (!unreleased) throw new Error('Missing Unreleased changelog section');
const notes = unreleased[1].trim() || 'Maintenance updates from the main branch.';
const date = new Date().toISOString().slice(0, 10);
updates.set('CHANGELOG.md', changelog.replace(unreleased[0], `## Unreleased\n\n## ${next} — ${date}\n\n${notes}\n`));
for (const [path, content] of updates) writeFileSync(path, content);
console.log(tag);
