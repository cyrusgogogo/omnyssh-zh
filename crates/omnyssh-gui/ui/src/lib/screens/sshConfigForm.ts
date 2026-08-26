import type { ManagedSshHostDto } from '$lib/bindings';

const ALIAS = /^[A-Za-z0-9._-]+$/;

export function validateManagedHost(host: ManagedSshHostDto): string | null {
  if (!ALIAS.test(host.alias)) return 'Alias may contain only ASCII letters, digits, ., _ and -';
  if (!host.hostname || /\s|[\u0000-\u001f\u007f]/u.test(host.hostname)) return 'HostName must be one value';
  if (!host.user || /\s|[\u0000-\u001f\u007f]/u.test(host.user)) return 'User must be one value';
  if (!Number.isInteger(host.port) || host.port < 1 || host.port > 65535) return 'Port must be between 1 and 65535';
  if (host.identityFile && !/^(?:~[/\\]|(?:[A-Za-z]:)?[/\\])/u.test(host.identityFile)) return 'IdentityFile must be absolute or start with ~';
  if (host.ciphers && /\s|[\u0000-\u001f\u007f]/u.test(host.ciphers)) return 'Ciphers must be one algorithm list without whitespace';
  return null;
}
