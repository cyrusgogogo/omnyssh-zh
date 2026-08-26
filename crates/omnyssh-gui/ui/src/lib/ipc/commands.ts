// Thin typed wrappers over the generated command bindings (tech-gui.md §3.5).
// Components call these, never `invoke` directly.

import type { Channel } from '@tauri-apps/api/core';
import { commands } from '$lib/bindings';
import { formatCommandError } from '$lib/i18n';
import type {
  FileEntryDto,
  HostDto,
  HostInputDto,
  ManagedSshHostDto,
  SshApplyReportDto,
  SshConfigPreviewDto,
  SshConfigSnapshotDto,
  SshKeyRecordDto,
  SshKeySnapshotDto,
  SnippetDto,
  TerminalBytes,
  UpdateConfigDto,
  UpdateInfoDto
} from '$lib/bindings';

export async function getSshKeySnapshot(): Promise<SshKeySnapshotDto> {
  const res = await commands.getSshKeySnapshot();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function createSshKey(
  name: string,
  fileName: string,
  keyType: string
): Promise<SshKeyRecordDto> {
  const res = await commands.createSshKey(name, fileName, keyType);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function importSshKey(name: string, privatePath: string): Promise<SshKeyRecordDto> {
  const res = await commands.importSshKey(name, privatePath);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function renameSshKey(id: string, name: string): Promise<void> {
  const res = await commands.renameSshKey(id, name);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function backupSshKey(id: string): Promise<string> {
  const res = await commands.backupSshKey(id);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function readSshPublicKey(id: string): Promise<string> {
  const res = await commands.readSshPublicKey(id);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function deleteSshKey(id: string, confirmation: string): Promise<string> {
  const res = await commands.deleteSshKey(id, confirmation);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function restoreSshKey(backupId: string): Promise<SshKeyRecordDto> {
  const res = await commands.restoreSshKey(backupId);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function loadTerminalOpenMode(): Promise<string> {
  const res = await commands.loadTerminalOpenMode();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function saveTerminalOpenMode(mode: string): Promise<void> {
  const res = await commands.saveTerminalOpenMode(mode);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function openSystemTerminal(hostName: string): Promise<void> {
  const res = await commands.openSystemTerminal(hostName);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function showDesktopCard(): Promise<void> {
  const res = await commands.showDesktopCard();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function setDesktopCardAlwaysOnTop(alwaysOnTop: boolean): Promise<boolean> {
  const res = await commands.setDesktopCardAlwaysOnTop(alwaysOnTop);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function openDesktopCardHost(
  hostName: string,
  kind: 'terminal' | 'sftp'
): Promise<void> {
  const res = await commands.openDesktopCardHost(hostName, kind);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function closeDesktopCard(): Promise<void> {
  const res = await commands.closeDesktopCard();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function getSshConfigSnapshot(): Promise<SshConfigSnapshotDto> {
  const res = await commands.getSshConfigSnapshot();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function previewSshConfig(hosts: ManagedSshHostDto[]): Promise<SshConfigPreviewDto> {
  const res = await commands.previewSshConfig(hosts);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function applySshConfig(
  hosts: ManagedSshHostDto[],
  expectedMainHash: string,
  expectedManagedHash: string,
  allowMissingSsh: boolean
): Promise<SshApplyReportDto> {
  const res = await commands.applySshConfig(
    hosts,
    expectedMainHash,
    expectedManagedHash,
    allowMissingSsh
  );
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function previewSshConfigRestore(backupId: string): Promise<SshConfigPreviewDto> {
  const res = await commands.previewSshConfigRestore(backupId);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

export async function restoreSshConfig(
  backupId: string,
  expectedMainHash: string,
  expectedManagedHash: string
): Promise<void> {
  const res = await commands.restoreSshConfig(backupId, expectedMainHash, expectedManagedHash);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

export async function listHosts(): Promise<HostDto[]> {
  const res = await commands.listHosts();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Reload hosts from disk and (re)start the pollers; broadcasts `hosts-loaded`. */
export async function reloadHosts(): Promise<void> {
  const res = await commands.reloadHosts();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Add or edit a manual host in `hosts.toml`. Call `reloadHosts` after to refresh. */
export async function saveHost(input: HostInputDto): Promise<void> {
  const res = await commands.saveHost(input);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Delete a manual host by name. A missing / SSH-config name is a no-op success. */
export async function deleteHost(name: string): Promise<void> {
  const res = await commands.deleteHost(name);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Read the saved snippets from the shared `snippets.toml`. */
export async function listSnippets(): Promise<SnippetDto[]> {
  const res = await commands.listSnippets();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Upsert one snippet by name and persist the whole list. */
export async function saveSnippet(snippet: SnippetDto): Promise<void> {
  const res = await commands.saveSnippet(snippet);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Delete the snippet named `name` and persist. */
export async function deleteSnippet(name: string): Promise<void> {
  const res = await commands.deleteSnippet(name);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Run a snippet on one or more hosts; results arrive as `snippet-result` events. */
export async function executeSnippet(
  snippetName: string,
  hostNames: string[],
  params: Record<string, string>
): Promise<void> {
  const res = await commands.executeSnippet(snippetName, hostNames, params);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Open a terminal for `hostName`, streaming raw output into `onOutput`; returns the
 *  public session id used by the write/resize/close wrappers (tech-gui.md §3.3/§4.2). */
export async function terminalOpen(
  hostName: string,
  cols: number,
  rows: number,
  onOutput: Channel<TerminalBytes>
): Promise<number> {
  const res = await commands.terminalOpen(hostName, cols, rows, onOutput);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Send keystrokes (UTF-8 bytes) to a terminal. */
export async function terminalWrite(sessionId: number, data: number[]): Promise<void> {
  const res = await commands.terminalWrite(sessionId, data);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Reflow a terminal to `cols` x `rows`. */
export async function terminalResize(sessionId: number, cols: number, rows: number): Promise<void> {
  const res = await commands.terminalResize(sessionId, cols, rows);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Close a terminal and its connection. Idempotent for an already-closed id. */
export async function terminalClose(sessionId: number): Promise<void> {
  const res = await commands.terminalClose(sessionId);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Open an SFTP session for `hostName`; returns the public session id the sftp_*
 *  wrappers use, and the tab's `sftp-*` events carry (tech-gui.md §3.4/§4.2). */
export async function sftpOpen(hostName: string): Promise<number> {
  const res = await commands.sftpOpen(hostName);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** List a remote directory; the result arrives as `sftp-dir-listed`. */
export async function sftpList(sessionId: number, path: string): Promise<void> {
  const res = await commands.sftpList(sessionId, path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Upload a local file to a remote path; progress arrives as `transfer-progress`. */
export async function sftpUpload(sessionId: number, local: string, remote: string): Promise<void> {
  const res = await commands.sftpUpload(sessionId, local, remote);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Download a remote file to a local path; progress arrives as `transfer-progress`. */
export async function sftpDownload(
  sessionId: number,
  local: string,
  remote: string
): Promise<void> {
  const res = await commands.sftpDownload(sessionId, local, remote);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Create a remote directory; completion arrives as `sftp-op-done`. */
export async function sftpMkdir(sessionId: number, path: string): Promise<void> {
  const res = await commands.sftpMkdir(sessionId, path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Rename / move a remote path; completion arrives as `sftp-op-done`. */
export async function sftpRename(sessionId: number, from: string, to: string): Promise<void> {
  const res = await commands.sftpRename(sessionId, from, to);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Delete a remote file (or empty directory); completion arrives as `sftp-op-done`. */
export async function sftpDelete(sessionId: number, path: string): Promise<void> {
  const res = await commands.sftpDelete(sessionId, path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Request a remote file preview; the bytes arrive as `file-preview`. */
export async function sftpPreview(sessionId: number, path: string): Promise<void> {
  const res = await commands.sftpPreview(sessionId, path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Close an SFTP session and its connection. Idempotent for an already-closed id. */
export async function sftpClose(sessionId: number): Promise<void> {
  const res = await commands.sftpClose(sessionId);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** List a local directory (returns directly — no event). */
export async function listLocalDir(path: string): Promise<FileEntryDto[]> {
  const res = await commands.listLocalDir(path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Read up to 4 KiB of a local file as UTF-8 for preview. */
export async function previewLocalFile(path: string): Promise<string> {
  const res = await commands.previewLocalFile(path);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Start auto SSH-key setup for a host; progress + the outcome arrive as `key-setup-*`
 *  events (tech-gui.md §4.2). Fire-and-forget — only an unknown host rejects here. */
export async function startKeySetup(hostName: string): Promise<void> {
  const res = await commands.startKeySetup(hostName);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Force an immediate metric poll of every host (tech-gui.md §4.2). */
export async function refreshMetrics(): Promise<void> {
  const res = await commands.refreshMetrics();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Check GitHub for a newer release; `null` means up to date (tech-gui.md §4.2). */
export async function checkUpdate(): Promise<UpdateInfoDto | null> {
  const res = await commands.checkUpdate();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Download and install the latest desktop bundle (tech-gui.md §4.3). Fully wired once
 *  Stage 5 configures the updater endpoints; until then it reports "not available yet". */
export async function installUpdate(): Promise<void> {
  const res = await commands.installUpdate();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}

/** Read the update-checker preferences from the shared config (tech-gui.md §4.3). */
export async function loadUpdateConfig(): Promise<UpdateConfigDto> {
  const res = await commands.loadUpdateConfig();
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
  return res.data;
}

/** Persist the update-checker preferences to the shared config (tech-gui.md §4.3). */
export async function saveUpdateConfig(config: UpdateConfigDto): Promise<void> {
  const res = await commands.saveUpdateConfig(config);
  if (res.status === 'error') throw new Error(formatCommandError(res.error));
}
