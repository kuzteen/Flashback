import { describe, expect, it } from 'vitest';
import { gameOverride, groupCover, sourceBadge } from './source-badge';

describe('sourceBadge', () => {
  it('a custom cover wins over everything', () => {
    expect(sourceBadge('VALORANT', 'cover.png', 'icon.png')).toEqual({ kind: 'cover', src: 'cover.png' });
    expect(sourceBadge('Pantalla 1', 'cover.png', null)).toEqual({ kind: 'cover', src: 'cover.png' });
  });

  it('without a cover the game icon shows', () => {
    expect(sourceBadge('VALORANT', null, 'icon.png')).toEqual({ kind: 'game', src: 'icon.png' });
  });

  it('a custom name without icon gets its initial', () => {
    expect(sourceBadge('  mi partida', null, null)).toEqual({ kind: 'initial', letter: 'M' });
  });

  it('screens and imported clips keep their own icons', () => {
    expect(sourceBadge('Pantalla 2', null, null)).toEqual({ kind: 'screen' });
    expect(sourceBadge('', null, null)).toEqual({ kind: 'imported' });
  });
});

describe('gameOverride', () => {
  it('picking the detected game again is not an override', () => {
    expect(gameOverride('valorant ', 'VALORANT')).toBeNull();
    expect(gameOverride('   ', 'VALORANT')).toBeNull();
  });

  it('another game or a custom name is stored trimmed', () => {
    expect(gameOverride(' Overwatch 2 ', 'VALORANT')).toBe('Overwatch 2');
    expect(gameOverride('mi partida', 'Pantalla 1')).toBe('mi partida');
  });
});

describe('groupCover', () => {
  const clip = (source: string, day: number, coverSrc: string | null) => ({
    source,
    coverSrc,
    createdAt: new Date(2026, 8, day)
  });

  it('uses the newest custom cover among the clips of that game', () => {
    const clips = [clip('Mi juego', 1, 'old.png'), clip('Mi juego', 3, 'new.png'), clip('Mi juego', 5, null)];
    expect(groupCover(clips, 'Mi juego')).toBe('new.png');
  });

  it('is null when no clip of that game has a cover', () => {
    const clips = [clip('Mi juego', 1, null), clip('Otro', 2, 'x.png')];
    expect(groupCover(clips, 'Mi juego')).toBeNull();
  });
});
