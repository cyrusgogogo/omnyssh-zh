import { describe, expect, it } from 'vitest';
import { sortCards } from './overview';
import { deriveCard } from '$lib/screens/serverCard';

function card(name: string, cpu?: number) {
  return deriveCard({ name, hostname: name, user: 'root', port: 22, source: 'manual',
    tags: [], hasKey: false, monitoring: 'ssh', hiddenFromOverview: false },
  { kind: 'connected' }, cpu === undefined ? undefined : { cpuPercent: cpu, ageSeconds: 0, topProcesses: [] }, undefined);
}

describe('overview sorting', () => {
  it('sorts names naturally without changing source order', () => {
    const cards = [card('host10'), card('host2'), card('host1')];
    expect(sortCards(cards, 'name', false).map(c => c.host.name)).toEqual(['host1', 'host2', 'host10']);
    expect(sortCards(cards, 'name', true).map(c => c.host.name)).toEqual(['host10', 'host2', 'host1']);
    expect(sortCards(cards, 'default', false)).toBe(cards);
    expect(cards[0].host.name).toBe('host10');
  });

  it('keeps absent and non-finite metrics last in both directions, with deterministic ties', () => {
    const cards = [card('missing'), card('high', 90), card('low2', 20), card('low1', 20), card('nan', NaN)];
    expect(sortCards(cards, 'cpu', true).map(c => c.host.name)).toEqual(['high', 'low1', 'low2', 'missing', 'nan']);
    expect(sortCards(cards, 'cpu', false).map(c => c.host.name)).toEqual(['low1', 'low2', 'high', 'missing', 'nan']);
  });

  it('supports severity, memory and disk usage', () => {
    const cards = [card('healthy', 10), card('warning', 70), card('critical', 95)];
    cards[0].metricRows[1].percent = 80;
    cards[1].metricRows[1].percent = 50;
    cards[2].metricRows[2].percent = 99;
    expect(sortCards(cards, 'status', true).map(c => c.host.name)).toEqual(['critical', 'warning', 'healthy']);
    expect(sortCards(cards, 'ram', true)[0].host.name).toBe('healthy');
    expect(sortCards(cards, 'disk', true)[0].host.name).toBe('critical');
  });
});
