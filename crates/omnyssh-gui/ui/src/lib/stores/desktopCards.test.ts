import { describe, expect, it } from 'vitest';
import {
  addDesktopCardHost,
  normalizeDesktopCardHosts,
  removeDesktopCardHost
} from './desktopCards';

describe('desktop card host selection', () => {
  it('keeps the first occurrence and removes invalid names', () => {
    expect(normalizeDesktopCardHosts([' alpha ', 'beta', 'alpha', '', 4])).toEqual([
      'alpha',
      'beta'
    ]);
  });

  it('adds without duplicating and removes only the selected host', () => {
    expect(addDesktopCardHost(['alpha'], 'alpha')).toEqual(['alpha']);
    expect(addDesktopCardHost(['alpha'], 'beta')).toEqual(['alpha', 'beta']);
    expect(removeDesktopCardHost(['alpha', 'beta'], 'alpha')).toEqual(['beta']);
  });
});
