import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, cpSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

test('patch release updates workspace versions and notes without changing dependency versions', () => {
  const dir = mkdtempSync(join(tmpdir(), 'omnyssh-release-test-'));
  try {
    for (const path of ['Cargo.toml', 'Cargo.lock', 'CHANGELOG.md', 'README.md', 'README.en.md', 'doc', 'crates/omnyssh/Cargo.toml']) {
      cpSync(path, join(dir, path), { recursive: true });
    }
    execFileSync('git', ['init'], { cwd: dir });
    const original = readFileSync(join(dir, 'Cargo.lock'), 'utf8');
    const current = readFileSync(join(dir, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)[1];
    const parts = current.split('.').map(Number);
    parts[2] += 1;
    const next = parts.join('.');
    const tag = execFileSync(process.execPath, [resolve('scripts/prepare-release.mjs')], { cwd: dir, encoding: 'utf8' }).trim();
    assert.equal(tag, `v${next}`);
    const lock = readFileSync(join(dir, 'Cargo.lock'), 'utf8');
    assert.equal(lock.replaceAll(`version = "${next}"`, `version = "${current}"`), original.replaceAll(`version = "${next}"`, `version = "${current}"`));
    assert.equal([...lock.matchAll(new RegExp(`name = "omnyssh(?:-core|-gui)?"\\r?\\nversion = "${next}"`, 'g'))].length, 3);
    assert.match(readFileSync(join(dir, 'CHANGELOG.md'), 'utf8'), new RegExp(`## Unreleased\\n\\n## ${next}`));
    execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '--allow-empty', '-m', 'fixture'], { cwd: dir });
    parts[2] += 1;
    execFileSync('git', ['tag', `v${parts.join('.')}`], { cwd: dir });
    assert.throws(() => execFileSync(process.execPath, [resolve('scripts/prepare-release.mjs')], { cwd: dir, stdio: 'pipe' }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
