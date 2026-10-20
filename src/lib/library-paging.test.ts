import { describe, expect, it, vi } from 'vitest';
import { ClipPager, type ClipPage, type LibraryState } from './library-paging';

type Clip = { id: string };
const clips = Array.from({ length: 205 }, (_, i) => ({ id: String(i) }));

function state(): LibraryState<Clip> {
  return { clips: [], total: 0, hasMore: false, loaded: false, loading: false, error: false };
}

function page(offset: number, limit: number | null): ClipPage<Clip> {
  const end = limit === null ? clips.length : offset + limit;
  return { clips: clips.slice(offset, end), total: clips.length, hasMore: end < clips.length };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe('library pagination', () => {
  it('opens with 100, appends batches, and refreshes the expanded window', async () => {
    const library = state();
    const fetch = vi.fn(async (offset: number, limit: number | null, _oldest: boolean) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.refresh();
    expect(fetch).toHaveBeenLastCalledWith(0, 100, false);
    expect(library.clips).toHaveLength(100);
    await pager.loadMore();
    expect(fetch).toHaveBeenLastCalledWith(100, 100, false);
    expect(library.clips).toHaveLength(200);
    await pager.refresh();
    expect(fetch).toHaveBeenLastCalledWith(0, 200, false);
    await pager.loadMore();
    expect(library.clips).toEqual(clips);
    expect(library.hasMore).toBe(false);
    expect(await pager.loadMore()).toBe(false);
  });

  it('keeps loaded clips and retries the same page after an error', async () => {
    const library = state();
    const fetch = vi.fn(async (offset: number, limit: number | null) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.refresh();
    fetch.mockRejectedValueOnce(new Error('offline'));
    expect(await pager.loadMore()).toBe(false);
    expect(library).toMatchObject({ error: true, loading: false, hasMore: true });
    expect(library.clips).toHaveLength(100);
    await pager.loadMore();
    expect(fetch).toHaveBeenLastCalledWith(100, 100, false);
    expect(library.clips).toHaveLength(200);
    expect(library.error).toBe(false);
  });

  it('ignores an old page arriving after a refresh or sort change', async () => {
    const library = state();
    const pending = deferred<ClipPage<Clip>>();
    const fetch = vi.fn(async (offset: number, limit: number | null) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.refresh();
    fetch.mockReturnValueOnce(pending.promise);
    const more = pager.loadMore();
    expect(await pager.loadMore()).toBe(false);
    await pager.refresh({ oldest: true, limit: 100 });
    pending.resolve(page(100, 100));
    expect(await more).toBe(false);
    expect(library.clips).toEqual(clips.slice(0, 100));
    expect(library.loading).toBe(false);
    expect(fetch).toHaveBeenLastCalledWith(0, 100, true);
  });

  it('deduplicates changing pages without reusing their offset', async () => {
    const library = state();
    const fetch = vi.fn(async (offset: number, limit: number | null) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.refresh();
    fetch.mockResolvedValueOnce({ clips: clips.slice(99, 199), total: 206, hasMore: true });
    await pager.loadMore();
    expect(library.clips).toHaveLength(199);
    await pager.loadMore();
    expect(fetch).toHaveBeenLastCalledWith(200, 100, false);
    expect(new Set(library.clips.map((c) => c.id)).size).toBe(library.clips.length);
  });

  it('loads older clips on demand for search and playlists, sharing an in-flight request', async () => {
    const library = state();
    const fetch = vi.fn(async (offset: number, limit: number | null) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.refresh();
    const pending = deferred<ClipPage<Clip>>();
    fetch.mockReturnValueOnce(pending.promise);
    const all = pager.loadAll();
    expect(pager.loadAll()).toBe(all);
    pending.resolve(page(0, null));
    await all;
    expect(library.clips).toEqual(clips);
    expect(fetch).toHaveBeenCalledTimes(2);
    await pager.refresh();
    expect(fetch).toHaveBeenLastCalledWith(0, null, false);
  });

  it('does not treat a previous full catalog as complete while a new window is loading', async () => {
    const library = state();
    const fetch = vi.fn(async (offset: number, limit: number | null) => page(offset, limit));
    const pager = new ClipPager(library, fetch);
    await pager.loadAll();
    const pending = deferred<ClipPage<Clip>>();
    fetch.mockReturnValueOnce(pending.promise);
    const firstPage = pager.refresh({ limit: 100 });
    await pager.loadAll();
    pending.resolve(page(0, 100));
    await firstPage;
    expect(library.clips).toEqual(clips);
    expect(library.hasMore).toBe(false);
  });
});
