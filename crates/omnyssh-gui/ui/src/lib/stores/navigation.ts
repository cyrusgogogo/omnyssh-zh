import { get } from 'svelte/store';
import { activeEntity } from './activeEntity';
import { sessions, type Session, type SessionKind } from './sessions';
import { terminalOpenMode } from './settings';
import { openSystemTerminal, openSftpCard } from '$lib/ipc/commands';
import { desktopSettings } from './desktopSettings';
import { lastError } from './notifications';

// Composed navigation actions that keep the sessions list and the active entity in
// step (tech-gui.md §2). A spawn appends a session and makes it active (both spawn
// paths do this); closing the active session falls back to the Dashboard so Content
// is never left pointing at a closed tab.
export function spawnSession(kind: SessionKind, hostName: string): Session {
  const session = sessions.spawn(kind, hostName);
  activeEntity.activateSession(session.id);
  return session;
}

/** Apply the user's preferred window for host actions. */
export function openHostSession(kind: SessionKind, hostName: string): Session | null {
  if (kind === 'sftp' && get(desktopSettings).sftpOpenMode === 'card') {
    void openSftpCard(hostName).catch((error) =>
      lastError.set(error instanceof Error ? error.message : String(error))
    );
    return null;
  }
  if (kind === 'terminal' && get(terminalOpenMode) === 'system') {
    void openSystemTerminal(hostName).catch((error) =>
      lastError.set(error instanceof Error ? error.message : String(error))
    );
    return null;
  }
  return spawnSession(kind, hostName);
}

export function closeSession(id: number): void {
  const active = get(activeEntity);
  if (active.kind === 'session' && active.id === id) {
    activeEntity.selectDashboard();
  }
  sessions.close(id);
}
