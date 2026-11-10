import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { collectPackages, createManifest, TARGETS, updaterConfig, verifyPackage } from './updater-release.mjs';

// Disposable signing keys stay in memory, never in the repository.
function signer() {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const id = randomBytes(8);
  const rawKey = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32);
  const keyLine = Buffer.concat([Buffer.from('Ed'), id, rawKey]).toString('base64');
  const key = Buffer.from(`untrusted comment: test public key\n${keyLine}\n`).toString('base64');
  return { key, signature(bytes, prehash = true) {
    const message = prehash ? createHash('blake2b512').update(bytes).digest() : bytes;
    const sig = sign(null, message, privateKey);
    const comment = 'timestamp:1\tfile:fixture';
    const global = sign(null, Buffer.concat([sig, Buffer.from(comment)]), privateKey);
    const encoded = Buffer.concat([Buffer.from(prehash ? 'ED' : 'Ed'), id, sig]).toString('base64');
    return Buffer.from(`untrusted comment: test signature\n${encoded}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`).toString('base64');
  } };
}

function workspace(t) {
  const root = mkdtempSync(join(tmpdir(), 'omnyssh-updater-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writeFileSync(join(root, 'Cargo.toml'), '[workspace.package]\nversion = "1.2.3"\n');
  return root;
}

test('configuration requires a Tauri public key and enables signed bundles', () => {
  for (const invalid of [undefined, '', 'not-a-key', Buffer.from('private key').toString('base64')]) {
    assert.throws(() => updaterConfig(invalid), /PUBLIC_KEY/);
  }
  const { key } = signer();
  assert.deepEqual(updaterConfig(key), { bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: key } } });
});

test('Minisign verification rejects damaged packages, comments and mismatched signing keys', () => {
  const one = signer(), two = signer(), bytes = Buffer.from('desktop installer');
  for (const prehash of [false, true]) {
    const signature = one.signature(bytes, prehash);
    verifyPackage(bytes, signature, one.key);
    assert.throws(() => verifyPackage(Buffer.from('tampered'), signature, one.key), /verification failed/);
    assert.throws(() => verifyPackage(bytes, signature, two.key), /does not match/);
    const tampered = Buffer.from(Buffer.from(signature, 'base64').toString().replace('timestamp:1', 'timestamp:2')).toString('base64');
    assert.throws(() => verifyPackage(bytes, tampered, one.key), /verification failed/);
  }
  assert.throws(() => verifyPackage(bytes, 'invalid', one.key), /Invalid updater signature/);
});

test('the manifest covers every GUI format, pins URLs and verifies all packages', (t) => {
  const root = workspace(t), signing = signer(), out = join(root, 'artifacts');
  for (const [target, specs] of Object.entries(TARGETS)) {
    for (const [, folder, suffix] of specs) {
      const dir = join(root, 'target', target, 'release', 'bundle', folder);
      mkdirSync(dir, { recursive: true });
      const bytes = Buffer.from(`desktop ${target} ${suffix}`);
      const file = join(dir, `OmnySSH_1.2.3${suffix}`);
      writeFileSync(file, bytes);
      writeFileSync(`${file}.sig`, signing.signature(bytes));
    }
    collectPackages(root, target, signing.key, out);
  }
  const manifest = createManifest(out, '1.2.3', signing.key, 'Release notes', '2026-11-09T00:00:00Z');
  assert.equal(Object.keys(manifest.platforms).length, 8);
  assert.equal(manifest.platforms['windows-x86_64'].url, 'https://github.com/cyrusgogogo/omnyssh-zh/releases/download/v1.2.3/OmnySSH-x86_64-setup.exe');
  assert.match(manifest.platforms['linux-x86_64-deb'].url, /\.deb$/);
  assert.match(manifest.platforms['linux-x86_64-rpm'].url, /\.rpm$/);
  assert.equal(manifest.notes, 'Release notes');
  assert.throws(() => createManifest(out, '1.2.4', signing.key), /version mismatch/);
  const installer = join(out, 'OmnySSH-x86_64-setup.exe');
  writeFileSync(installer, Buffer.concat([readFileSync(installer), Buffer.from('modified')]));
  assert.throws(() => createManifest(out, '1.2.3', signing.key), /verification failed/);
});

test('collection rejects missing signatures and stale package versions', (t) => {
  const root = workspace(t), signing = signer(), target = 'x86_64-pc-windows-msvc';
  const dir = join(root, 'target', target, 'release', 'bundle', 'nsis');
  mkdirSync(dir, { recursive: true });
  const stale = join(dir, 'OmnySSH_1.2.2-setup.exe');
  writeFileSync(stale, 'old installer');
  assert.throws(() => collectPackages(root, target, signing.key), /Wrong package version/);
  rmSync(stale);
  writeFileSync(join(dir, 'OmnySSH_1.2.3-setup.exe'), 'current installer');
  assert.throws(() => collectPackages(root, target, signing.key), /ENOENT/);
});
