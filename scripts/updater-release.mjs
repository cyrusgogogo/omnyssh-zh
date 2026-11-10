// Build and publish Tauri's signed update packages alongside the normal installers.
import { createHash, createPublicKey, verify } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const REPO = 'cyrusgogogo/omnyssh-zh';
export const TARGETS = {
  'aarch64-apple-darwin': [
    ['darwin-aarch64', 'macos', '.app.tar.gz', 'OmnySSH-aarch64-apple-darwin.app.tar.gz']
  ],
  'x86_64-apple-darwin': [
    ['darwin-x86_64', 'macos', '.app.tar.gz', 'OmnySSH-x86_64-apple-darwin.app.tar.gz']
  ],
  'x86_64-unknown-linux-gnu': [
    ['linux-x86_64-appimage', 'appimage', '.AppImage', 'OmnySSH-x86_64.AppImage'],
    ['linux-x86_64-deb', 'deb', '.deb', 'OmnySSH-x86_64.deb'],
    ['linux-x86_64-rpm', 'rpm', '.rpm', 'OmnySSH-x86_64.rpm']
  ],
  'x86_64-pc-windows-msvc': [
    ['windows-x86_64-nsis', 'nsis', '-setup.exe', 'OmnySSH-x86_64-setup.exe']
  ]
};

function decodePublicKey(encoded) {
  const lines = Buffer.from(encoded ?? '', 'base64').toString('utf8').trim().split(/\r?\n/);
  const key = Buffer.from(lines[1] ?? '', 'base64');
  if (!lines[0]?.startsWith('untrusted comment: ') || key.length !== 42 || key.subarray(0, 2).toString() !== 'Ed') {
    throw new Error('TAURI_UPDATER_PUBLIC_KEY must contain the base64 public key produced by tauri signer generate.');
  }
  return key;
}

export function updaterConfig(publicKey) {
  decodePublicKey(publicKey);
  return {
    bundle: { createUpdaterArtifacts: true },
    plugins: { updater: { pubkey: publicKey.trim() } }
  };
}

// Fail the release if the public key and private signing secret do not match, or
// if a package/signature was corrupted. This is the same Minisign format Tauri verifies.
export function verifyPackage(bytes, encodedSignature, encodedPublicKey) {
  const pub = decodePublicKey(encodedPublicKey);
  const lines = Buffer.from(encodedSignature, 'base64').toString('utf8').trim().split(/\r?\n/);
  const sig = Buffer.from(lines[1] ?? '', 'base64');
  const global = Buffer.from(lines[3] ?? '', 'base64');
  if (sig.length !== 74 || global.length !== 64 || !lines[2]?.startsWith('trusted comment: ')) {
    throw new Error('Invalid updater signature.');
  }
  const algorithm = sig.subarray(0, 2).toString();
  if (!['Ed', 'ED'].includes(algorithm) || !pub.subarray(2, 10).equals(sig.subarray(2, 10))) {
    throw new Error('Updater signing key does not match TAURI_UPDATER_PUBLIC_KEY.');
  }
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), pub.subarray(10)]),
    type: 'spki', format: 'der'
  });
  const signature = sig.subarray(10);
  const message = algorithm === 'ED' ? createHash('blake2b512').update(bytes).digest() : bytes;
  if (!verify(null, message, key, signature) ||
      !verify(null, Buffer.concat([signature, Buffer.from(lines[2].slice(17))]), key, global)) {
    throw new Error('Updater package signature verification failed.');
  }
}

function versionOf(root) {
  const version = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)?.[1];
  if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) throw new Error('Missing stable workspace version.');
  return version;
}

export function collectPackages(root, target, publicKey, out = root) {
  const specs = TARGETS[target];
  if (!specs) throw new Error(`Unsupported updater target: ${target}`);
  const version = versionOf(root);
  const platforms = {};
  mkdirSync(out, { recursive: true });
  for (const [platform, folder, suffix, name] of specs) {
    const bundleDir = join(root, 'target', target, 'release', 'bundle', folder);
    const files = readdirSync(bundleDir).filter((name) => name.endsWith(suffix));
    if (files.length !== 1) throw new Error(`Expected one ${suffix} updater package, found ${files.length}.`);
    const file = join(bundleDir, files[0]);
    if (suffix !== '.app.tar.gz' && !files[0].includes(version)) throw new Error(`Wrong package version: ${files[0]}`);
    const bytes = readFileSync(file);
    if (!bytes.length) throw new Error(`Empty updater package: ${file}`);
    const signature = readFileSync(`${file}.sig`, 'utf8').trim();
    verifyPackage(bytes, signature, publicKey);
    copyFileSync(file, join(out, name));
    writeFileSync(join(out, `${name}.sig`), `${signature}\n`);
    platforms[platform] = { name, signature };
  }
  writeFileSync(join(out, `updater-${target}.json`), JSON.stringify({ version, platforms }, null, 2));
}

export function createManifest(artifacts, version, publicKey, notes = '', publishedAt = new Date().toISOString()) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Invalid updater version.');
  const platforms = {};
  for (const [target, specs] of Object.entries(TARGETS)) {
    const part = JSON.parse(readFileSync(join(artifacts, `updater-${target}.json`), 'utf8'));
    if (part.version !== version) throw new Error(`Updater version mismatch for ${target}.`);
    for (const [platform, , , name] of specs) {
      const entry = part.platforms[platform];
      if (!entry || entry.name !== name || basename(entry.name) !== entry.name) throw new Error(`Missing updater platform: ${platform}`);
      const path = join(artifacts, name);
      if (!statSync(path).size) throw new Error(`Empty updater asset: ${name}`);
      const signature = readFileSync(`${path}.sig`, 'utf8').trim();
      if (signature !== entry.signature) throw new Error(`Updater signature mismatch: ${name}`);
      verifyPackage(readFileSync(path), signature, publicKey);
      platforms[platform] = { signature, url: `https://github.com/${REPO}/releases/download/v${version}/${name}` };
    }
  }
  // Fallback for older clients whose bundle type is not included in their target.
  platforms['windows-x86_64'] = platforms['windows-x86_64-nsis'];
  platforms['linux-x86_64'] = platforms['linux-x86_64-appimage'];
  return { version, notes, pub_date: publishedAt, platforms };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [command, arg] = process.argv.slice(2);
  const publicKey = process.env.TAURI_UPDATER_PUBLIC_KEY;
  if (command === 'configure') {
    if (!process.env.TAURI_SIGNING_PRIVATE_KEY) throw new Error('Configure the TAURI_SIGNING_PRIVATE_KEY Actions secret before publishing.');
    mkdirSync(dirname(arg), { recursive: true });
    writeFileSync(arg, JSON.stringify(updaterConfig(publicKey), null, 2));
  } else if (command === 'collect') {
    collectPackages(process.cwd(), arg, publicKey);
  } else if (command === 'manifest') {
    const version = versionOf(process.cwd());
    const changelog = readFileSync('CHANGELOG.md', 'utf8');
    const notes = changelog.split(`## ${version} `)[1]?.split('\n').slice(1).join('\n').split(/^## |^---$/m)[0]?.trim() ?? '';
    writeFileSync(join(arg, 'latest.json'), JSON.stringify(createManifest(arg, version, publicKey, notes), null, 2));
  } else {
    throw new Error('Usage: updater-release.mjs configure <config.json> | collect <target> | manifest <artifacts>');
  }
}
