import { describe, expect, it } from 'vitest';
import type { ManagedSshHostDto } from '$lib/bindings';
import { validateManagedHost } from './sshConfigForm';

const host = (alias = 'prod'): ManagedSshHostDto => ({
  alias,
  hostname: 'prod.example.com',
  user: 'deploy',
  port: 22,
  identityFile: null,
  proxyJump: null,
  ciphers: null
});

describe('SSH config form', () => {
  it('rejects pattern and injection aliases', () => {
    expect(validateManagedHost(host('*'))).not.toBeNull();
    expect(validateManagedHost(host('bad\nHost evil'))).not.toBeNull();
  });

  it('accepts an exact safe host', () => {
    expect(validateManagedHost(host('prod-1.example'))).toBeNull();
  });
});
