import { describe, expect, it, vi } from 'vitest';
import { keyFileNameFromBytes, randomKeyFileName } from './sshKeyForm';

describe('SSH key filename defaults', () => {
  it('renders a stable lowercase hexadecimal suffix from random bytes', () => {
    expect(keyFileNameFromBytes(Uint8Array.from([0x00, 0x1a, 0x2b, 0xff]))).toBe(
      'id_omnyssh_001a2bff'
    );
  });

  it('requests four random bytes instead of returning the old fixed filename', () => {
    const random = vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation((array) => {
      (array as Uint8Array).set([0xde, 0xad, 0xbe, 0xef]);
      return array;
    });

    expect(randomKeyFileName()).toBe('id_omnyssh_deadbeef');
    expect(random).toHaveBeenCalledOnce();
    random.mockRestore();
  });
});
