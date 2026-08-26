const RANDOM_SUFFIX_BYTES = 4;

/** Build the default private-key filename from cryptographically random bytes. */
export function keyFileNameFromBytes(bytes: Uint8Array): string {
  const suffix = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return `id_omnyssh_${suffix}`;
}

/** A collision-resistant default that remains recognizable inside ~/.ssh/. */
export function randomKeyFileName(): string {
  const bytes = new Uint8Array(RANDOM_SUFFIX_BYTES);
  globalThis.crypto.getRandomValues(bytes);
  return keyFileNameFromBytes(bytes);
}
