import { describe, expect, it } from 'vitest';
import { exportStage } from './export-stage';

describe('exportStage', () => {
  it('follows the real progress', () => {
    expect(exportStage(0)).toBe('ed.stage.prepare');
    expect(exportStage(0.2)).toBe('ed.stage.trim');
    expect(exportStage(0.5)).toBe('ed.stage.audio');
    expect(exportStage(0.8)).toBe('ed.stage.encode');
    expect(exportStage(0.96)).toBe('ed.stage.finish');
    expect(exportStage(1)).toBe('ed.stage.finish');
  });
});
