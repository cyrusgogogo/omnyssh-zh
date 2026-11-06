import { describe, expect, it } from 'vitest';
import type { FileEntryDto } from '$lib/bindings';
import { fileExtension, formatModified, sortEntries, type FileSort } from './sftpListing';

const entry = (name: string, options: Partial<FileEntryDto> = {}): FileEntryDto =>
  ({ name, path: `/home/${name}`, size: 0, isDir: false, ...options });

describe('file listing order', () => {
  const files = [
    entry('file10.txt', { size: 30, modified: 300 }),
    entry('file2.zip', { size: 10, modified: 200 }),
    entry('file1.log', { size: 20, modified: 100 })
  ];

  it.each<[FileSort, string[]]>([
    ['name', ['file1.log', 'file2.zip', 'file10.txt']],
    ['modified', ['file1.log', 'file2.zip', 'file10.txt']],
    ['type', ['file1.log', 'file10.txt', 'file2.zip']],
    ['size', ['file2.zip', 'file1.log', 'file10.txt']]
  ])('sorts by %s in both directions without mutating the source', (sort, names) => {
    const original = [...files];
    expect(sortEntries(files, { sort, descending: false }).map((e) => e.name)).toEqual(names);
    expect(sortEntries(files, { sort, descending: true }).map((e) => e.name)).toEqual([...names].reverse());
    expect(files).toEqual(original);
  });

  it('keeps the parent and folders first and unavailable dates last in both directions', () => {
    const entries = [entry('unknown'), ...files, entry('folder', { isDir: true }), entry('..', { isDir: true })];
    for (const descending of [false, true]) {
      const sorted = sortEntries(entries, { sort: 'modified', descending });
      expect(sorted.slice(0, 2).map((e) => e.name)).toEqual(['..', 'folder']);
      expect(sorted.at(-1)?.name).toBe('unknown');
    }
  });

  it('does not change entry identities used by marks and transfers', () => {
    const marked = new Set([files[0].path]);
    const sorted = sortEntries(files, { sort: 'size', descending: false });
    expect(sorted.filter((e) => marked.has(e.path))).toEqual([files[0]]);
  });

  it('treats dotted folder names as the same folder type', () => {
    const entries = [entry('folder.log', { isDir: true }), entry('alpha.zip', { isDir: true })];
    expect(sortEntries(entries, { sort: 'type', descending: false }).map((e) => e.name))
      .toEqual(['alpha.zip', 'folder.log']);
  });
});

describe('file metadata', () => {
  it('handles hidden files, compound names, extension case and missing extensions', () => {
    expect(fileExtension('.env')).toBe('');
    expect(fileExtension('README')).toBe('');
    expect(fileExtension('trailing.')).toBe('');
    expect(fileExtension('archive.tar.GZ')).toBe('gz');
    expect(fileExtension('.config.json')).toBe('json');
  });

  it('formats Unix seconds, including epoch zero, and leaves unknown values empty', () => {
    expect(formatModified(1_700_000_000, 'zh-CN')).toContain('2023');
    expect(formatModified(0, 'en-US')).toContain('1970');
    for (const value of [undefined, null, NaN, Infinity, Number.MAX_VALUE]) {
      expect(formatModified(value, 'en-US')).toBe('—');
    }
  });
});
